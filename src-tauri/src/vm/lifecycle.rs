//! VM lifecycle — RoboLaunch
//! Reconstruit et corrigé depuis VibeStarter.

use anyhow::{Context, Result};
use base64::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use russh_keys::PublicKeyBase64;
use tracing::info;
use uuid::Uuid;

use crate::agent::{AgentInfo, AgentStatus, AgentType};
use crate::mcp::gateway::ToolboxItem;
use super::ssh::SshClient;

pub const GUEST_IP:   &str = "10.0.2.15";
pub const HOST_ALIAS: &str = "10.0.2.2";
pub const GUEST_USER: &str = "app-user";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmStatus {
    pub running:   bool,
    pub ssh_ready: bool,
    pub pid:       Option<u32>,
}

#[derive(Debug, Clone)]
pub struct VmManager(Arc<Inner>);

#[derive(Debug)]
struct Inner {
    resource_dir: PathBuf,
    data_dir:     PathBuf,
    state:        RwLock<State>,
    ssh_key:      RwLock<Option<russh_keys::key::KeyPair>>,
}

#[derive(Debug, Default)]
struct State {
    pid:           Option<u32>,
    ssh_ready:     bool,
    ssh_host_port: u16,
    agents:        Vec<AgentInfo>,
    toolbox:       Vec<ToolboxItem>,
}

impl VmManager {
    pub fn new(resource_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self(Arc::new(Inner {
            resource_dir,
            data_dir,
            state:   RwLock::new(State::default()),
            ssh_key: RwLock::new(None),
        }))
    }

    pub async fn start(&self) -> Result<()> {
        let vm_dir   = self.0.resource_dir.join("vm");
        let data_dir = &self.0.data_dir;

        // Clé SSH éphémère
        let key = russh_keys::key::KeyPair::generate_ed25519()
            .context("generate SSH keypair")?;
        let pubkey_b64 = {
            let line = format!("ssh-ed25519 {} robolaunch-host", key.public_key_base64());
            base64::engine::general_purpose::STANDARD.encode(line)
        };
        *self.0.ssh_key.write().await = Some(key);

        // Volume persistant
        let qcow2 = data_dir.join("user-volume.qcow2");
        if !qcow2.exists() {
            create_qcow2(&vm_dir, &qcow2, "20G").await?;
        }

        let ssh_port = free_port().await?;

        // Lance gvproxy (syntaxe correcte)
        launch_gvproxy(&vm_dir, ssh_port).await?;

        // Lance QEMU
        let child = launch_qemu(&vm_dir, &qcow2, &pubkey_b64, ssh_port).await?;
        let pid = child.id();

        let mut s = self.0.state.write().await;
        s.pid           = pid;
        s.ssh_host_port = ssh_port;
        info!("QEMU PID={pid:?}, SSH→localhost:{ssh_port}");
        Ok(())
    }

    pub async fn wait_ssh_ready(&self, timeout_secs: u64) -> Result<()> {
        let port     = self.0.state.read().await.ssh_host_port;
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_secs(timeout_secs);
        while std::time::Instant::now() < deadline {
            if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                self.0.state.write().await.ssh_ready = true;
                info!("SSH ready on port {port}");
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        anyhow::bail!("SSH not ready after {timeout_secs}s")
    }

    pub async fn ssh(&self) -> Result<SshClient> {
        let port = self.0.state.read().await.ssh_host_port;
        let key  = self.0.ssh_key.read().await.clone()
            .context("SSH key not generated yet")?;
        SshClient::connect("127.0.0.1", port, GUEST_USER, key).await
    }

    pub async fn status(&self) -> VmStatus {
        let s = self.0.state.read().await;
        VmStatus { running: s.pid.is_some(), ssh_ready: s.ssh_ready, pid: s.pid }
    }

    // ─── Agents ───────────────────────────────────────────────────────────────

    pub async fn list_agents(&self) -> Result<Vec<AgentInfo>> {
        Ok(self.0.state.read().await.agents.clone())
    }

    pub async fn launch_agent(
        &self,
        agent_type: &str,
        name: &str,
        gateway_port: u16,
        roblox_port: u16,
        roblox_secret: &str,
    ) -> Result<String> {
        let id  = Uuid::new_v4().to_string();
        let num = self.0.state.read().await.agents.len() + 1;

        let token: String = (0..32)
            .map(|_| rand::thread_rng().gen::<u8>())
            .map(|b| format!("{b:02x}"))
            .collect();

        let binary = agent_type.parse::<AgentType>()?.binary();

        // Échappement basique pour insertion entre apostrophes dans la
        // commande shell distante (le nom vient de l'UI, donc potentiellement
        // arbitraire — une apostrophe non échappée casserait la commande).
        let safe_name = name.replace('\'', "'\\''");

        // Env vars: VIBESTARTER_ → ROBOLAUNCH_ pour compatibilité mcp-stub interne
        // On garde les deux préfixes pour compatibilité avec mcp-stub.cjs original
        let cmd = format!(
            "systemd-run \
             --uid={GUEST_USER} \
             --slice=user-1000.slice \
             --unit=robolaunch-agent-{id}.scope \
             --scope \
             -E ROBOLAUNCH_AGENT_ID={id} \
             -E ROBOLAUNCH_AGENT_NUM={num} \
             -E ROBOLAUNCH_AGENT_NAME='{safe_name}' \
             -E ROBOLAUNCH_TAB_LABEL='{safe_name}' \
             -E ROBOLAUNCH_AGENT_TYPE={agent_type} \
             -E ROBOLAUNCH_GATEWAY_HOST={HOST_ALIAS} \
             -E ROBOLAUNCH_GATEWAY_PORT={gateway_port} \
             -E ROBOLAUNCH_AGENT_MCP_TOKEN={token} \
             -E ROBOLAUNCH_ROBLOX_MCP_HOST={HOST_ALIAS} \
             -E ROBOLAUNCH_ROBLOX_MCP_PORT={roblox_port} \
             -E ROBOLAUNCH_ROBLOX_MCP_SECRET={roblox_secret} \
             -E VIBESTARTER_AGENT_ID={id} \
             -E VIBESTARTER_GATEWAY_HOST={HOST_ALIAS} \
             -E VIBESTARTER_GATEWAY_PORT={gateway_port} \
             -E VIBESTARTER_AGENT_MCP_TOKEN={token} \
             -E VIBESTARTER_ROBLOX_MCP_HOST={HOST_ALIAS} \
             -E VIBESTARTER_ROBLOX_MCP_PORT={roblox_port} \
             -E VIBESTARTER_ROBLOX_MCP_SECRET={roblox_secret} \
             -E HOME=/home/app-user \
             -- /bin/bash -l -c '{binary}'"
        );

        let ssh = self.ssh().await?;
        ssh.exec(&cmd).await?;

        self.0.state.write().await.agents.push(AgentInfo {
            id:         id.clone(),
            name:       name.to_string(),
            agent_type: agent_type.to_string(),
            status:     AgentStatus::Running,
            tab_label:  name.to_string(),
            mcp_token:  token,
        });
        info!("Launched agent {id} type={agent_type}");
        Ok(id)
    }

    pub async fn pause_agent(&self, id: &str) -> Result<()> {
        let ssh  = self.ssh().await?;
        let unit = format!("robolaunch-agent-{id}.scope");
        ssh.exec(&format!("systemctl freeze '{unit}'")).await?;
        self.set_status(id, AgentStatus::Paused).await;
        Ok(())
    }

    pub async fn resume_agent(&self, id: &str) -> Result<()> {
        let ssh  = self.ssh().await?;
        let unit = format!("robolaunch-agent-{id}.scope");
        ssh.exec(&format!("systemctl thaw '{unit}'")).await?;
        self.set_status(id, AgentStatus::Running).await;
        Ok(())
    }

    pub async fn kill_agent(&self, id: &str) -> Result<()> {
        let ssh  = self.ssh().await?;
        let unit = format!("robolaunch-agent-{id}.scope");
        ssh.exec(&format!("systemctl stop '{unit}'")).await?;
        self.0.state.write().await.agents.retain(|a| a.id != id);
        Ok(())
    }

    // ─── Toolbox ──────────────────────────────────────────────────────────────

    pub async fn list_toolbox(&self) -> Result<Vec<ToolboxItem>> {
        Ok(self.0.state.read().await.toolbox.clone())
    }

    pub async fn add_toolbox_item(&self, item: ToolboxItem) {
        self.0.state.write().await.toolbox.push(item);
    }

    async fn set_status(&self, id: &str, status: AgentStatus) {
        let mut s = self.0.state.write().await;
        if let Some(a) = s.agents.iter_mut().find(|a| a.id == id) {
            a.status = status;
        }
    }
}

// ─── Helpers QEMU ─────────────────────────────────────────────────────────────

async fn create_qcow2(vm_dir: &Path, out: &Path, size: &str) -> Result<()> {
    let bin = vm_dir.join("qemu-img.exe");
    let st  = Command::new(&bin)
        .args(["create", "-f", "qcow2", out.to_str().expect("chemin qcow2 non-UTF-8"), size])
        .status().await?;
    anyhow::ensure!(st.success(), "qemu-img create failed");
    info!("qcow2 created: {}", out.display());
    Ok(())
}

async fn launch_gvproxy(vm_dir: &Path, ssh_port: u16) -> Result<()> {
    let bin = vm_dir.join("gvproxy.exe");
    // Syntaxe correcte gvproxy: -listen + -forward-sock séparés
    Command::new(&bin)
        .args([
            "-listen",       "vsock://:1024",
            "-forward-sock", &format!("tcp://127.0.0.1:{ssh_port}"),
            "-forward-dest", &format!("{GUEST_IP}:22"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

async fn launch_qemu(
    vm_dir: &Path, qcow2: &Path, authkey_b64: &str, _ssh_port: u16,
) -> Result<tokio::process::Child> {
    let qemu     = vm_dir.join("qemu-system-x86_64.exe");
    let squashfs = vm_dir.join("robolaunch-guest.squashfs");
    let vmlinuz  = vm_dir.join("vmlinuz");
    let initrd   = vm_dir.join("initrd.img");

    let cmdline = format!(
        "root=/dev/vda ro console=ttyS0 robolaunch.authkey={authkey_b64}"
    );

    let child = Command::new(&qemu)
        .args([
            "-nographic",
            "-m", "4096",
            "-smp", "4",
            "-kernel", vmlinuz.to_str().expect("chemin vmlinuz non-UTF-8"),
            "-initrd", initrd.to_str().expect("chemin initrd non-UTF-8"),
            "-append", &cmdline,
            "-drive", &format!(
                "file={},format=raw,if=virtio,readonly=on",
                squashfs.display()
            ),
            "-drive", &format!(
                "file={},format=qcow2,if=virtio",
                qcow2.display()
            ),
            "-netdev", "socket,id=net0,connect=127.0.0.1:1024",
            "-device", "virtio-net-pci,netdev=net0",
            "-device", "virtio-rng-pci",
            "-display", "none",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(child)
}

async fn free_port() -> Result<u16> {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    Ok(l.local_addr()?.port())
}
