//! HTTP Gateway MCP — RoboLaunch
//! Endpoints: /mcp/health, /mcp/rpc, /mcp/upload-file, /mcp/close,
//!            /mcp/notify (nouveau — notifications système tray)

use anyhow::Result;
use axum::{
    body::Bytes,
    extract::{Multipart, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::vm::lifecycle::VmManager;

// ─── Types publics ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolboxItem {
    pub id:          String,
    pub name:        String,
    pub description: String,
    pub category:    String,
}

/// Requête envoyée au bridge Roblox depuis la gateway
pub struct RobloxReq {
    pub body:     serde_json::Value,
    pub reply_tx: oneshot::Sender<serde_json::Value>,
}

#[derive(Clone)]
pub struct GatewayState {
    pub vm:          VmManager,
    pub roblox_tx:   Arc<RwLock<Option<mpsc::Sender<RobloxReq>>>>,
    /// Tokens MCP valides: token → agent_id
    pub tokens:      Arc<RwLock<HashMap<String, String>>>,
    /// Fichiers temporaires uploadés: server_file_id → bytes
    pub file_store:  Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

// ─── Démarrage ────────────────────────────────────────────────────────────────

pub async fn start_gateway(vm: VmManager) -> Result<(u16, GatewayState)> {
    let state = GatewayState {
        vm,
        roblox_tx:  Arc::new(RwLock::new(None)),
        tokens:     Arc::new(RwLock::new(HashMap::new())),
        file_store: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/mcp/health",      get(health))
        .route("/mcp/rpc",         post(rpc))
        .route("/mcp/upload-file", post(upload_file))
        .route("/mcp/close",       post(close_agent))
        .route("/mcp/notify",      post(notify))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    info!("Gateway HTTP démarré sur port={port}");
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Ok((port, state))
}

// ─── Authentification ─────────────────────────────────────────────────────────

async fn auth_agent(headers: &HeaderMap, state: &GatewayState) -> Option<String> {
    let token = headers.get("X-RoboLaunch-Token")
        .or_else(|| headers.get("X-VibeStarter-Token")) // compat
        .and_then(|v| v.to_str().ok())?
        .trim_start_matches("Bearer ")
        .to_string();
    state.tokens.read().await.get(&token).cloned()
}

/// Enregistre un token pour un agent (appelé après launch_agent)
pub async fn register_token(state: &GatewayState, agent_id: &str, token: &str) {
    state.tokens.write().await.insert(token.to_string(), agent_id.to_string());
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "app": "RoboLaunch" }))
}

async fn rpc(
    State(st): State<GatewayState>,
    headers:   HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    debug!("MCP RPC: {body}");

    let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("");

    // Tools internes RoboLaunch
    if method.starts_with("robolaunch/") {
        return handle_internal(&st, method, &body).await;
    }

    // Forward Roblox Studio
    let lock = st.roblox_tx.read().await;
    if let Some(tx) = lock.as_ref() {
        let (reply_tx, reply_rx) = oneshot::channel();
        let req = RobloxReq { body, reply_tx };
        if tx.send(req).await.is_ok() {
            match tokio::time::timeout(
                std::time::Duration::from_secs(30),
                reply_rx,
            ).await {
                Ok(Ok(resp)) => return (StatusCode::OK, Json(resp)).into_response(),
                _ => {}
            }
        }
    }

    (StatusCode::SERVICE_UNAVAILABLE,
     Json(serde_json::json!({ "error": "Roblox Studio non connecté" })))
        .into_response()
}

async fn handle_internal(
    st: &GatewayState,
    method: &str,
    _body: &serde_json::Value,
) -> axum::response::Response {
    match method {
        "robolaunch/listToolbox" => {
            let items = st.vm.list_toolbox().await.unwrap_or_default();
            (StatusCode::OK, Json(serde_json::json!({ "result": items }))).into_response()
        }
        "robolaunch/vmStatus" => {
            let status = st.vm.status().await;
            (StatusCode::OK, Json(serde_json::json!({ "result": status }))).into_response()
        }
        _ => (StatusCode::NOT_FOUND,
              Json(serde_json::json!({ "error": "Méthode inconnue" }))).into_response()
    }
}

async fn upload_file(
    State(st): State<GatewayState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if auth_agent(&headers, &st).await.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response();
    }
    while let Ok(Some(field)) = multipart.next_field().await {
        if let Ok(data) = field.bytes().await {
            let id = Uuid::new_v4().to_string();
            info!("File upload: {id} ({} bytes)", data.len());
            st.file_store.write().await.insert(id.clone(), data.to_vec());
            return (StatusCode::OK, Json(serde_json::json!({ "serverFileId": id }))).into_response();
        }
    }
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"No file"}))).into_response()
}

async fn close_agent(
    State(st): State<GatewayState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(agent_id) = auth_agent(&headers, &st).await {
        info!("Agent {agent_id} se déconnecte");
        // Nettoyer le token
        let token = headers.get("X-RoboLaunch-Token")
            .or_else(|| headers.get("X-VibeStarter-Token"))
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .trim_start_matches("Bearer ")
            .to_string();
        st.tokens.write().await.remove(&token);
    }
    StatusCode::NO_CONTENT
}

// Nouveau — notifications push vers le host (système tray ou toast Windows)
#[derive(Deserialize)]
struct NotifyPayload {
    title:   String,
    message: String,
}

async fn notify(
    State(st): State<GatewayState>,
    headers: HeaderMap,
    Json(payload): Json<NotifyPayload>,
) -> impl IntoResponse {
    if auth_agent(&headers, &st).await.is_none() {
        return StatusCode::UNAUTHORIZED;
    }
    info!("Notification agent: [{}] {}", payload.title, payload.message);
    // TODO: intégrer tauri::notification::Notification dans v2
    StatusCode::OK
}
