use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
// use std::time::Instant;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot, Mutex};
use uuid::Uuid;

use crate::agent::protocol::{AgentMessage, ControllerMessage, ExecutionPayload};
use crate::executor::{ExecuteRequest, ExecuteResponse, Executor, ExecutorError, ExecutorResult};

pub mod config;

// ============================================================================
// SSH Configuration
// ============================================================================

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SshConfig {
    pub host: String,
    pub user: String,
    pub port: u16,
    pub identity: Option<String>,
}

#[allow(dead_code)]
impl SshConfig {
    pub fn new(host: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            user: user.into(),
            port: 22,
            identity: None,
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn with_identity(mut self, identity: impl Into<String>) -> Self {
        self.identity = Some(identity.into());
        self
    }
}

// ============================================================================
// Shared pending-request map
// request_id → oneshot sender that resolves the execute() future
// ============================================================================

type PendingMap = Arc<Mutex<HashMap<String, oneshot::Sender<ExecutorResult<ExecuteResponse>>>>>;

// ============================================================================
// Remote Executor
// ============================================================================

pub struct RemoteExecutor {
    name: String,
    tx: mpsc::Sender<ExecutionPayload>,
    pending: PendingMap,
    _handle: Arc<tokio::task::JoinHandle<()>>,
}

impl RemoteExecutor {
    pub async fn new(config: SshConfig) -> ExecutorResult<Self> {
        let name = format!("SSH:{}@{}", config.user, config.host);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = mpsc::channel::<ExecutionPayload>(100);

        let pending_bg = pending.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = Self::ssh_task(config, rx, pending_bg).await {
                eprintln!("[VOLT] SSH task exited: {}", e);
            }
        });

        Ok(Self {
            name,
            tx,
            pending,
            _handle: Arc::new(handle),
        })
    }

    // -----------------------------------------------------------------------
    // Background task: owns the SSH subprocess, routes messages in/out
    // -----------------------------------------------------------------------

    async fn ssh_task(
        config: SshConfig,
        mut rx: mpsc::Receiver<ExecutionPayload>,
        pending: PendingMap,
    ) -> ExecutorResult<()> {
        // Build: ssh -o StrictHostKeyChecking=no [-p PORT] [-i IDENTITY] user@host volt --agent
        let mut cmd = Command::new("ssh");
        cmd.arg("-o").arg("StrictHostKeyChecking=no");
        cmd.arg("-o").arg("UserKnownHostsFile=/dev/null");

        if config.port != 22 {
            cmd.arg("-p").arg(config.port.to_string());
        }
        if let Some(ref identity) = config.identity {
            cmd.arg("-i").arg(identity);
        }

        cmd.arg(format!("{}@{}", config.user, config.host));
        cmd.arg("volt").arg("--agent");

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null()); // silence SSH banner noise

        let mut child = cmd.spawn().map_err(|e| {
            ExecutorError::RemoteConnectionError(format!("SSH spawn failed: {}", e))
        })?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| ExecutorError::RemoteConnectionError("No SSH stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ExecutorError::RemoteConnectionError("No SSH stdout".into()))?;

        // ---- Reader task: SSH stdout → resolve pending oneshots ----
        let pending_reader = pending.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<AgentMessage>(&line) {
                    Ok(AgentMessage::ExecutionResult(result)) => {
                        let mut map = pending_reader.lock().await;
                        if let Some(tx) = map.remove(&result.request_id) {
                            let response = ExecuteResponse {
                                status: result.status,
                                headers: result.headers,
                                body: result.body,
                                duration_ms: result.duration_ms,
                                size_bytes: result.size_bytes,
                            };
                            let _ = tx.send(Ok(response));
                        }
                    }
                    Ok(AgentMessage::HealthOk) => {
                        eprintln!("[VOLT] Remote agent healthy");
                    }
                    Ok(AgentMessage::Error(e)) => {
                        eprintln!("[VOLT] Agent error: {}", e);
                    }
                    Ok(AgentMessage::Shutdown) => break,
                    Err(e) => {
                        eprintln!("[VOLT] Unreadable agent response: {} — raw: {}", e, line);
                    }
                }
            }

            // SSH stdout closed — fail every pending request
            let mut map = pending_reader.lock().await;
            for (_, tx) in map.drain() {
                let _ = tx.send(Err(ExecutorError::RemoteConnectionError(
                    "SSH connection lost".into(),
                )));
            }
        });

        // ---- Writer loop: pending map → SSH stdin ----
        while let Some(payload) = rx.recv().await {
            let msg = ControllerMessage::Execute(payload);
            match serde_json::to_string(&msg) {
                Ok(json) => {
                    let line = format!("{}\n", json);
                    if let Err(e) = stdin.write_all(line.as_bytes()).await {
                        eprintln!("[VOLT] SSH stdin write error: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("[VOLT] Serialization error: {}", e);
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// Executor impl
// ============================================================================

#[async_trait::async_trait]
impl Executor for RemoteExecutor {
    async fn execute(&self, req: ExecuteRequest) -> ExecutorResult<ExecuteResponse> {
        let request_id = Uuid::new_v4().to_string();

        let payload = ExecutionPayload {
            request_id: request_id.clone(),
            method: req.method,
            url: req.url,
            headers: req.headers,
            query_params: req.query_params,
            body: req.body,
            timeout_ms: req.timeout_ms,
        };

        // Register a oneshot before sending — avoids a race where the
        // response arrives before we've inserted the sender
        let (response_tx, response_rx) = oneshot::channel();
        {
            let mut map = self.pending.lock().await;
            map.insert(request_id.clone(), response_tx);
        }

        // Send payload to SSH writer loop
        if let Err(e) = self.tx.send(payload).await {
            // Remove the dangling oneshot entry
            self.pending.lock().await.remove(&request_id);
            return Err(ExecutorError::RemoteConnectionError(format!(
                "Send to SSH task failed: {}",
                e
            )));
        }

        // Wait for the reader task to resolve our oneshot, with a timeout
        let timeout = std::time::Duration::from_millis(
            req.timeout_ms.unwrap_or(30_000) + 5_000, // a little headroom
        );

        tokio::time::timeout(timeout, response_rx)
            .await
            .map_err(|_| {
                // Timed out — clean up pending entry
                let pending = self.pending.clone();
                let rid = request_id.clone();
                tokio::spawn(async move {
                    pending.lock().await.remove(&rid);
                });
                ExecutorError::RetryableError("Remote request timed out".into())
            })?
            .map_err(|_| ExecutorError::RemoteConnectionError("Response channel dropped".into()))?
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    async fn healthcheck(&self) -> ExecutorResult<()> {
        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_config_builder() {
        let config = SshConfig::new("example.com", "ubuntu")
            .with_port(2222)
            .with_identity("~/.ssh/id_ed25519");

        assert_eq!(config.host, "example.com");
        assert_eq!(config.user, "ubuntu");
        assert_eq!(config.port, 2222);
        assert!(config.identity.is_some());
    }
}
