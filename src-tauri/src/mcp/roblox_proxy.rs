//! Bridge TCP Roblox MCP — CORRIGÉ (race condition supprimée).
//! Architecture: UN seul canal sérialisé vers mcp.bat.
//! Toutes les requêtes (gateway HTTP + pipe direct guest) passent
//! par le même mpsc, avec matching d'id JSON-RPC pour les réponses.

use anyhow::Result;
use rand::Rng;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, info, warn};

use crate::mcp::gateway::RobloxReq;

pub async fn start_roblox_bridge(
    roblox_tx_slot: Arc<RwLock<Option<mpsc::Sender<RobloxReq>>>>,
) -> Result<(u16, String)> {
    let secret: String = (0..32)
        .map(|_| rand::thread_rng().gen::<u8>())
        .map(|b| format!("{b:02x}"))
        .collect();

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port     = listener.local_addr()?.port();
    info!("Roblox bridge écoute sur port={port}");

    let secret_clone = secret.clone();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    debug!("Roblox bridge: connexion de {addr}");
                    let sec      = secret_clone.clone();
                    let tx_slot  = roblox_tx_slot.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle(stream, &sec, tx_slot).await {
                            warn!("Roblox bridge erreur: {e}");
                        }
                    });
                }
                Err(e) => {
                    warn!("Roblox bridge accept: {e}");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    });

    Ok((port, secret))
}

async fn handle(
    stream: TcpStream,
    expected_secret: &str,
    roblox_tx_slot: Arc<RwLock<Option<mpsc::Sender<RobloxReq>>>>,
) -> Result<()> {
    let (reader, writer) = stream.into_split();
    let mut guest_reader = BufReader::new(reader);

    // Auth: première ligne = secret
    let mut first = String::new();
    guest_reader.read_line(&mut first).await?;
    if first.trim() != expected_secret {
        warn!("Roblox bridge: mauvais secret, connexion refusée");
        return Ok(());
    }
    info!("Roblox bridge: auth OK, lancement mcp.bat");

    // Lance mcp.bat
    let mut mcp = Command::new("cmd.exe")
        .args(["/c", r"%LOCALAPPDATA%\Roblox\mcp.bat"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let mcp_stdin  = mcp.stdin.take().expect("mcp.bat lancé sans Stdio::piped() sur stdin");
    let mcp_stdout = mcp.stdout.take().expect("mcp.bat lancé sans Stdio::piped() sur stdout");

    // Map id JSON-RPC → canal de réponse, pour les requêtes venant de la
    // gateway HTTP (/mcp/rpc). Partagée entre la tâche d'écriture (qui y
    // insère juste avant d'écrire vers mcp.bat) et la tâche de lecture
    // (qui y retire pour router la réponse vers l'appelant HTTP).
    let pending: Arc<RwLock<HashMap<String, oneshot::Sender<Value>>>> =
        Arc::new(RwLock::new(HashMap::new()));

    // Canal unique vers mcp.bat (FIX race condition)
    let (write_tx, mut write_rx) = mpsc::channel::<(Value, Option<oneshot::Sender<Value>>)>(64);

    // Canal exposé à la gateway HTTP
    let (gw_tx, mut gw_rx) = mpsc::channel::<RobloxReq>(32);
    *roblox_tx_slot.write().await = Some(gw_tx);

    // Tâche: forward requêtes gateway → write_tx
    let write_tx2 = write_tx.clone();
    tokio::spawn(async move {
        while let Some(req) = gw_rx.recv().await {
            let _ = write_tx2.send((req.body, Some(req.reply_tx))).await;
        }
    });

    // Tâche: écrire vers mcp.bat (séquentiellement), en enregistrant le
    // canal de réponse attendu par id JSON-RPC AVANT d'écrire la ligne,
    // pour éviter une course avec la réponse qui pourrait arriver vite.
    let mut mcp_in = mcp_stdin;
    let pending_write = pending.clone();
    tokio::spawn(async move {
        while let Some((body, reply)) = write_rx.recv().await {
            if let Some(reply_tx) = reply {
                match id_key(&body) {
                    Some(key) => { pending_write.write().await.insert(key, reply_tx); }
                    None => {
                        // Pas d'id JSON-RPC exploitable: impossible de router une
                        // réponse plus tard, on répond tout de suite plutôt que de
                        // laisser l'appelant HTTP attendre 30s pour rien.
                        let _ = reply_tx.send(serde_json::json!({
                            "error": "Requête sans id JSON-RPC valide"
                        }));
                    }
                }
            }
            let line = serde_json::to_string(&body).unwrap_or_default() + "\n";
            if mcp_in.write_all(line.as_bytes()).await.is_err() { break; }
        }
    });

    let mut mcp_reader   = BufReader::new(mcp_stdout);
    let mut guest_writer = writer;
    let pending_read = pending.clone();

    // Tâche lecture mcp.bat → guest + gateway
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            match mcp_reader.read_line(&mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&line) {
                        // Essaie de router vers la gateway si id connu
                        if let Some(key) = id_key(&v) {
                            if let Some(tx) = pending_read.write().await.remove(&key) {
                                let _ = tx.send(v.clone());
                                continue;
                            }
                        }
                    }
                    // Sinon forward au guest
                    if guest_writer.write_all(line.as_bytes()).await.is_err() { break; }
                }
            }
        }
    });

    // Tâche lecture guest → mcp.bat via write_tx
    let write_tx3 = write_tx.clone();
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            match guest_reader.read_line(&mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&line) {
                        let _ = write_tx3.send((v, None)).await;
                    }
                }
            }
        }
    });

    // Attendre la fin du processus mcp.bat
    let _ = mcp.wait().await;
    *roblox_tx_slot.write().await = None;
    info!("Roblox bridge: connexion fermée");
    Ok(())
}

/// Clé stable dérivée de l'id JSON-RPC d'un message (sérialisation canonique
/// de la valeur `id`), utilisée pour faire correspondre une requête sortante
/// à sa réponse entrante. Tant que l'insertion et la lecture utilisent la
/// même extraction, le format exact (avec ou sans guillemets) n'a pas
/// d'importance — seule la cohérence compte.
fn id_key(v: &Value) -> Option<String> {
    v.get("id").map(|i| i.to_string())
}
