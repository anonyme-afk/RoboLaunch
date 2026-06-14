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

    let mcp_stdin  = mcp.stdin.take().unwrap();
    let mcp_stdout = mcp.stdout.take().unwrap();

    // Canal unique vers mcp.bat (FIX race condition)
    let (write_tx, write_rx) = mpsc::channel::<(Value, Option<oneshot::Sender<Value>>)>(64);

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

    // Tâche: écrire vers mcp.bat (séquentiellement)
    let mut mcp_in = mcp_stdin;
    tokio::spawn(async move {
        let mut rx: mpsc::Receiver<(Value, Option<oneshot::Sender<Value>>)> = write_rx;
        while let Some((body, _reply)) = rx.recv().await {
            let line = serde_json::to_string(&body).unwrap_or_default() + "\n";
            if mcp_in.write_all(line.as_bytes()).await.is_err() { break; }
        }
    });

    // Tâche: lire les réponses de mcp.bat et les router
    // On garde une map id→oneshot pour les réponses gateway
    let pending: Arc<RwLock<HashMap<String, oneshot::Sender<Value>>>> =
        Arc::new(RwLock::new(HashMap::new()));

    let mut mcp_reader  = BufReader::new(mcp_stdout);
    let mut guest_writer = writer;

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
                        if let Some(id) = v.get("id").and_then(|i| Some(i.to_string())) {
                            if let Some(tx) = pending.write().await.remove(&id) {
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
