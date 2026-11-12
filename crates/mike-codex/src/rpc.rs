use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, Semaphore, oneshot};

use super::CodexError;
use super::events::EventSender;
use super::rpc_frame::{Frame, read_frame};

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
    call_slots: Semaphore,
    reader: tokio::task::JoinHandle<()>,
}

impl AppServer {
    pub(crate) async fn spawn(
        command: &ServerCommand,
        events: EventSender,
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
        let stdin = Arc::new(Mutex::new(stdin));
        let pending = Arc::default();
        let reader = tokio::spawn(read_lines(
            BufReader::new(stdout),
            Arc::clone(&pending),
            Arc::clone(&stdin),
            events,
        ));
        let server = Self {
            child,
            stdin,
            pending,
            next_id: AtomicI64::new(1),
            call_slots: Semaphore::new(32),
            reader,
        };
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
        let _permit = self
            .call_slots
            .acquire()
            .await
            .map_err(|_| CodexError::Closed("RPC capacity"))?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.retain(|_, sender| !sender.is_closed());
            pending.insert(id, tx);
        }
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if let Err(error) = write_line(&self.stdin, &message).await {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        let reply = wait_reply(&self.pending, id, method, rx).await?;
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

impl Drop for AppServer {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

async fn wait_reply(
    pending: &Pending,
    id: i64,
    method: &str,
    receiver: oneshot::Receiver<Value>,
) -> Result<Value, CodexError> {
    let result = tokio::time::timeout(CALL_DEADLINE, receiver)
        .await
        .map_err(|_| CodexError::Timeout(method.to_string()))
        .and_then(|reply| reply.map_err(|_| CodexError::Closed("stdout")));
    pending.lock().await.remove(&id);
    result
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
    events: EventSender,
) {
    let mut stdout = stdout;
    loop {
        match read_frame(&mut stdout).await {
            Ok(Frame::Data(line)) => match serde_json::from_slice::<Value>(&line) {
                Ok(message) => route(message, &pending, &stdin, &events).await,
                Err(_) => events.gap("invalid JSON app-server frame"),
            },
            Ok(Frame::Oversized) => events.gap("app-server frame exceeds 8 MiB"),
            Ok(Frame::End) => break,
            Err(_) => {
                events.gap("app-server stdout read failed");
                break;
            }
        }
    }
    pending.lock().await.clear();
}

async fn route(message: Value, pending: &Pending, stdin: &Mutex<ChildStdin>, events: &EventSender) {
    let id = message.get("id").and_then(Value::as_i64);
    match (id, message.get("method")) {
        (Some(id), None) => route_response(pending, id, message).await,
        (Some(_), Some(_)) => refuse_server_request(&message, stdin).await,
        (None, Some(_)) => {
            events.send(message);
        }
        (None, None) => {}
    }
}

async fn route_response(pending: &Pending, id: i64, message: Value) {
    if let Some(waiter) = pending.lock().await.remove(&id) {
        let _ = waiter.send(message);
    }
}

async fn refuse_server_request(message: &Value, stdin: &Mutex<ChildStdin>) {
    let reply = json!({
        "jsonrpc": "2.0",
        "id": message["id"],
        "error": {
            "code": NO_INTERACTIVE_APPROVAL,
            "message": format!(
                "this app-server client has no interactive request handler; refused {}",
                message["method"]
            ),
        },
    });
    let _ = write_line(stdin, &reply).await;
}

#[cfg(test)]
#[path = "rpc_tests.rs"]
mod tests;
