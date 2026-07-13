use crate::error::{AppError, ErrorKind};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

fn tunnel_err(m: impl std::fmt::Display) -> AppError {
    AppError { kind: ErrorKind::TunnelFailed, message: m.to_string(), detail: None }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HostKeyOutcome {
    Pending,
    /// no fingerprint on record — first use; the value should be persisted
    New(String),
    /// matched the recorded fingerprint
    Matched,
    /// recorded fingerprint differs — possible MITM; connection is refused
    Mismatch { expected: String, got: String },
}

/// Verifies the server key against a recorded fingerprint (trust-on-first-use).
struct HostKeyVerifier {
    expected: Option<String>,
    outcome: Arc<Mutex<HostKeyOutcome>>,
}

#[async_trait::async_trait]
impl russh::client::Handler for HostKeyVerifier {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        let fp = server_public_key.fingerprint();
        let mut out = self.outcome.lock().await;
        match &self.expected {
            None => {
                *out = HostKeyOutcome::New(fp);
                Ok(true) // first use: accept and record
            }
            Some(exp) if *exp == fp => {
                *out = HostKeyOutcome::Matched;
                Ok(true)
            }
            Some(exp) => {
                *out = HostKeyOutcome::Mismatch { expected: exp.clone(), got: fp };
                Ok(false) // refuse: the host key changed
            }
        }
    }
}

pub struct SshAuth<'a> {
    /// Some => publickey auth (secret is the optional passphrase); None => password auth
    pub key_path: Option<&'a str>,
    pub secret: Option<&'a str>,
}

#[derive(Debug)]
pub struct SshTunnel {
    pub local_port: u16,
    /// Some(fingerprint) when a key was trusted on first use and should be persisted.
    pub new_host_key: Option<String>,
    task: tokio::task::JoinHandle<()>,
}

impl SshTunnel {
    pub async fn open(
        ssh_host: &str,
        ssh_port: u16,
        ssh_user: &str,
        auth: SshAuth<'_>,
        target_host: &str,
        target_port: u16,
        expected_host_key: Option<String>,
    ) -> Result<SshTunnel, AppError> {
        let config = Arc::new(russh::client::Config::default());
        let outcome = Arc::new(Mutex::new(HostKeyOutcome::Pending));
        let verifier = HostKeyVerifier { expected: expected_host_key, outcome: outcome.clone() };
        let connect = russh::client::connect(config, (ssh_host, ssh_port), verifier).await;

        // a host-key mismatch surfaces as a connect failure — turn it into a clear error
        let observed = outcome.lock().await.clone();
        if let HostKeyOutcome::Mismatch { expected, got } = &observed {
            return Err(tunnel_err(format!(
                "SSH host key for {ssh_host}:{ssh_port} changed — refusing to connect (possible \
                 man-in-the-middle). If the server key was legitimately rotated, forget the old \
                 key and reconnect.\nexpected {expected}\ngot      {got}"
            )));
        }
        let mut session = connect.map_err(tunnel_err)?;
        let new_host_key = match observed {
            HostKeyOutcome::New(fp) => Some(fp),
            _ => None,
        };

        let authed = match auth.key_path {
            Some(path) => {
                let key = russh_keys::load_secret_key(path, auth.secret).map_err(tunnel_err)?;
                session
                    .authenticate_publickey(ssh_user, Arc::new(key))
                    .await
                    .map_err(tunnel_err)?
            }
            None => session
                .authenticate_password(ssh_user, auth.secret.unwrap_or(""))
                .await
                .map_err(tunnel_err)?,
        };
        if !authed {
            return Err(tunnel_err("SSH authentication failed"));
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(tunnel_err)?;
        let local_port = listener.local_addr().map_err(tunnel_err)?.port();
        let target_host = target_host.to_string();

        let task = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else { break };
                // a failed channel open (e.g. DB briefly down) must not kill the
                // tunnel: drop this client connection and keep listening
                let Ok(channel) = session
                    .channel_open_direct_tcpip(&target_host, target_port as u32, "127.0.0.1", 0)
                    .await
                else {
                    continue;
                };
                tokio::spawn(async move {
                    let mut stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                });
            }
        });
        Ok(SshTunnel { local_port, new_host_key, task })
    }
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Default)]
pub struct TunnelRegistry {
    inner: Mutex<HashMap<String, SshTunnel>>,
}

impl TunnelRegistry {
    pub async fn insert(&self, id: &str, t: SshTunnel) {
        self.inner.lock().await.insert(id.to_string(), t);
    }
    pub async fn remove(&self, id: &str) {
        self.inner.lock().await.remove(id);
    }
    pub async fn remove_by_connection(&self, id: &str) {
        let suffix = format!(":{id}");
        self.inner.lock().await.retain(|k, _| !k.ends_with(&suffix));
    }
    pub async fn remove_by_window(&self, label: &str) {
        let prefix = format!("{label}:");
        self.inner.lock().await.retain(|k, _| !k.starts_with(&prefix));
    }
}
