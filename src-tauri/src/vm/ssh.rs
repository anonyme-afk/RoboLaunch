//! Client SSH via russh — contrôle la VM depuis Tauri.

use anyhow::{Context, Result};
use async_trait::async_trait;
use russh::client::{self, Handler};
use russh_keys::key::KeyPair;
use std::sync::Arc;
use tokio::net::TcpStream;
use tracing::debug;

pub struct SshClient {
    session: client::Handle<GuestHandler>,
}

struct GuestHandler;

#[async_trait]
impl Handler for GuestHandler {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        _key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true) // clé générée par nous-mêmes au boot
    }
}

impl SshClient {
    pub async fn connect(host: &str, port: u16, user: &str, key: KeyPair) -> Result<Self> {
        let cfg = Arc::new(client::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(300)),
            ..<_>::default()
        });
        let stream = TcpStream::connect((host, port))
            .await
            .with_context(|| format!("TCP {host}:{port}"))?;
        let mut sess = client::connect(cfg, stream, GuestHandler)
            .await
            .context("SSH handshake")?;
        let ok = sess
            .authenticate_publickey(user, Arc::new(key))
            .await
            .context("SSH auth")?;
        anyhow::ensure!(ok, "SSH auth rejeté pour {user}");
        debug!("SSH ok: {user}@{host}:{port}");
        Ok(Self { session: sess })
    }

    pub async fn exec(&self, cmd: &str) -> Result<(String, String, u32)> {
        debug!("ssh exec: {cmd}");
        let mut ch = self.session.channel_open_session().await?;
        ch.exec(true, cmd).await?;
        let (mut out, mut err, mut code) = (Vec::new(), Vec::new(), 0u32);
        loop {
            match ch.wait().await {
                None => break,
                Some(russh::ChannelMsg::Data { data })                 => out.extend_from_slice(&data),
                Some(russh::ChannelMsg::ExtendedData { data, ext: 1 }) => err.extend_from_slice(&data),
                Some(russh::ChannelMsg::ExitStatus { exit_status })    => code = exit_status,
                Some(russh::ChannelMsg::Eof)                           => break,
                Some(_) => {}
            }
        }
        Ok((
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&err).into_owned(),
            code,
        ))
    }

    pub async fn open_pty(
        &self, cmd: &str, cols: u32, rows: u32,
    ) -> Result<russh::Channel<russh::client::Msg>> {
        let mut ch = self.session.channel_open_session().await?;
        ch.request_pty(true, "xterm-256color", cols, rows, 0, 0, &[]).await?;
        ch.exec(true, cmd).await?;
        Ok(ch)
    }
}
