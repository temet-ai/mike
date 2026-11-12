use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, mpsc, oneshot};

use super::CodexError;

pub(crate) const CALL_DEADLINE: Duration = Duration::from_secs(60);
const NO_INTERACTIVE_APPROVAL: i64 = -32601;

type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>;

#[derive(Debug, Clone)]
pub(crate) struct ServerCommand {
    pub program: PathBuf,
    pub cwd: PathBuf,
    pub codex_home: PathBuf,
    pub env: Vec<(String, String)>,
}

pub(crate) struct AppServer {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    next_id: AtomicI64,
}

impl AppServer {
    pub(crate) async fn spawn(
        command: &ServerCommand,
        events: mpsc::UnboundedSender<Value>,
    ) -> Result<Self, CodexError> {
        let mut child = Command::new(&command.program)
            .arg("app-server")
            .current_dir(&command.cwd)
            .env("CODEX_HOME", &command.codex_home)
            .envs(command.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| CodexError::Spawn {
                program: command.program.display().to_string(),
                error: error.to_string(),
            })?;
        let stdin = child.stdin.take().ok_or(CodexError::Closed("stdin"))?;
        let stdout = child.stdout.take().ok_or(CodexError::Closed("stdout"))?;
        let server = Self {
            child,
            stdin: Arc::new(Mutex::new(stdin)),
            pending: Arc::default(),
            next_id: AtomicI64::new(1),
        };
        tokio::spawn(read_lines(
            BufReader::new(stdout),
            server.pending.clone(),
            server.stdin.clone(),
            events,
        ));
        server.initialize().await?;
        Ok(server)
    }

    async fn initialize(&self) -> Result<(), CodexError> {
        self.call(
            "initialize",
            json!({"clientInfo": {"name": "mike", "version": env!("CARGO_PKG_VERSION")}}),
        )
        .await?;
        write_line(
            &self.stdin,
            &json!({"jsonrpc": "2.0", "method": "initialized"}),
        )
        .await
    }

    pub(crate) async fn call(&self, method: &str, params: Value) -> Result<Value, CodexError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        write_line(&self.stdin, &message).await?;
        let reply = tokio::time::timeout(CALL_DEADLINE, rx)
            .await
            .map_err(|_| CodexError::Timeout(method.to_string()))?
            .map_err(|_| CodexError::Closed("stdout"))?;
        match reply.get("error") {
            Some(error) => Err(CodexError::Rpc {
                method: method.to_string(),
                params: message["params"].to_string(),
                error: error.to_string(),
            }),
            None => Ok(reply.get("result").cloned().unwrap_or(Value::Null)),
        }
    }

    pub(crate) fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    pub(crate) async fn stop(mut self) -> Result<(), CodexError> {
        self.child
            .kill()
            .await
            .map_err(|error| CodexError::Stop(error.to_string()))
    }
}

async fn write_line(stdin: &Mutex<ChildStdin>, message: &Value) -> Result<(), CodexError> {
    let mut line = message.to_string();
    line.push('\n');
    let mut stdin = stdin.lock().await;
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|_| CodexError::Closed("stdin"))?;
    stdin.flush().await.map_err(|_| CodexError::Closed("stdin"))
}

async fn read_lines(
    stdout: BufReader<tokio::process::ChildStdout>,
    pending: Pending,
    stdin: Arc<Mutex<ChildStdin>>,
    events: mpsc::UnboundedSender<Value>,
) {
    let mut lines = stdout.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        route(message, &pending, &stdin, &events).await;
    }
    pending.lock().await.clear();
}

async fn route(
    message: Value,
    pending: &Pending,
    stdin: &Mutex<ChildStdin>,
    events: &mpsc::UnboundedSender<Value>,
) {
    let id = message.get("id").and_then(Value::as_i64);
    match (id, message.get("method")) {
        (Some(id), None) => {
            if let Some(waiter) = pending.lock().await.remove(&id) {
                let _ = waiter.send(message);
            }
        }
        (Some(_), Some(_)) => refuse_server_request(&message, stdin).await,
        (None, Some(_)) => {
            let _ = events.send(message);
        }
        (None, None) => {}
    }
}

async fn refuse_server_request(message: &Value, stdin: &Mutex<ChildStdin>) {
    let reply = json!({
        "jsonrpc": "2.0",
        "id": message["id"],
        "error": {
            "code": NO_INTERACTIVE_APPROVAL,
            "message": format!(
                "mike fleet seats run with approvalPolicy never and answer no server request; refused {}",
                message["method"]
            ),
        },
    });
    let _ = write_line(stdin, &reply).await;
}
