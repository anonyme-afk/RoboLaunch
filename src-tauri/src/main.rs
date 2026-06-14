#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod mcp;
mod vm;

use anyhow::Result;
use tauri::Manager;
use tracing::info;

pub struct AppState {
    pub vm:            vm::lifecycle::VmManager,
    pub gateway_port:  u16,
    pub roblox_port:   u16,
    pub roblox_secret: String,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "robo_launch=debug,info".into()),
        )
        .init();

    info!("RoboLaunch démarrage");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = bootstrap(handle).await {
                    tracing::error!("Bootstrap échoué: {e:#}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd_vm_status,
            cmd_list_agents,
            cmd_launch_agent,
            cmd_pause_agent,
            cmd_resume_agent,
            cmd_kill_agent,
            cmd_list_toolbox,
        ])
        .run(tauri::generate_context!())
        .expect("Erreur Tauri");
}

async fn bootstrap(app: tauri::AppHandle) -> Result<()> {
    use mcp::gateway::start_gateway;
    use mcp::roblox_proxy::start_roblox_bridge;
    use vm::lifecycle::VmManager;

    let res_dir  = app.path().resource_dir()?;
    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;

    let vm = VmManager::new(res_dir, data_dir);
    vm.start().await?;
    info!("VM lancée");
    vm.wait_ssh_ready(60).await?;
    info!("SSH prêt");

    let (gateway_port, gw_state) = start_gateway(vm.clone()).await?;
    info!("Gateway MCP port={gateway_port}");

    let (roblox_port, roblox_secret) = start_roblox_bridge(gw_state.roblox_tx.clone()).await?;
    info!("Bridge Roblox port={roblox_port}");

    app.manage(AppState { vm, gateway_port, roblox_port, roblox_secret });
    info!("Bootstrap terminé ✓");
    Ok(())
}

// ─── Commandes Tauri ──────────────────────────────────────────────────────────

#[tauri::command]
async fn cmd_vm_status(state: tauri::State<'_, AppState>) -> Result<vm::lifecycle::VmStatus, String> {
    Ok(state.vm.status().await)
}

#[tauri::command]
async fn cmd_list_agents(state: tauri::State<'_, AppState>) -> Result<Vec<agent::AgentInfo>, String> {
    state.vm.list_agents().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn cmd_launch_agent(
    state: tauri::State<'_, AppState>,
    agent_type: String,
    name: String,
) -> Result<String, String> {
    state.vm.launch_agent(
        &agent_type, &name,
        state.gateway_port, state.roblox_port, &state.roblox_secret,
    ).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn cmd_pause_agent(state: tauri::State<'_, AppState>, agent_id: String) -> Result<(), String> {
    state.vm.pause_agent(&agent_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn cmd_resume_agent(state: tauri::State<'_, AppState>, agent_id: String) -> Result<(), String> {
    state.vm.resume_agent(&agent_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn cmd_kill_agent(state: tauri::State<'_, AppState>, agent_id: String) -> Result<(), String> {
    state.vm.kill_agent(&agent_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn cmd_list_toolbox(state: tauri::State<'_, AppState>) -> Result<Vec<mcp::gateway::ToolboxItem>, String> {
    state.vm.list_toolbox().await.map_err(|e| e.to_string())
}
