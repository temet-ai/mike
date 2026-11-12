use std::io::{BufRead, Write};

use serde_json::{Value, json};

#[derive(Default)]
struct Server {
    thread: String,
    turn: u64,
    approval_request: Option<Value>,
}

pub fn run() -> anyhow::Result<()> {
    let mut server = Server::default();
    for line in std::io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        if request["method"] == "fixture/close" {
            break;
        }
        server.handle(&request)?;
    }
    Ok(())
}

fn send(message: Value) -> anyhow::Result<()> {
    let mut output = std::fs::OpenOptions::new().write(true).open("/dev/fd/3")?;
    writeln!(output, "{message}")?;
    output.flush()?;
    Ok(())
}

fn reply(request: &Value, result: Value) -> anyhow::Result<()> {
    send(json!({"id": request["id"], "result": result}))
}

impl Server {
    fn handle(&mut self, request: &Value) -> anyhow::Result<()> {
        let params = &request["params"];
        match request["method"].as_str() {
            Some("initialize") => reply(request, json!({})),
            Some("initialized") => Ok(()),
            Some("thread/start" | "thread/resume" | "thread/fork") => self.thread(request),
            Some("model/list") => reply(request, json!({"data": [{"model": "fixture-model"}]})),
            Some("turn/start") => self.start_turn(request),
            Some("turn/interrupt") => {
                reply(request, json!({}))?;
                self.complete(&params["turnId"], "interrupted")
            }
            Some("fixture/refuse") => send(
                json!({"id": request["id"], "error": {"code": -1, "message": "fixture refusal"}}),
            ),
            Some("fixture/approval") => self.approval(request),
            None => self.approval_response(request),
            other => anyhow::bail!("unexpected fixture method: {other:?}"),
        }
    }

    fn thread(&mut self, request: &Value) -> anyhow::Result<()> {
        let params = &request["params"];
        if params["developerInstructions"] == "missing thread id" {
            return reply(request, json!({"thread": {}}));
        }
        self.thread = match request["method"].as_str() {
            Some("thread/resume") => params["threadId"].as_str().unwrap_or_default().into(),
            Some("thread/fork") => {
                format!("{}-fork", params["threadId"].as_str().unwrap_or_default())
            }
            _ => "thread-fixture".into(),
        };
        reply(request, json!({"thread": {"id": self.thread}}))
    }

    fn start_turn(&mut self, request: &Value) -> anyhow::Result<()> {
        self.turn += 1;
        let turn = format!("turn-{}", self.turn);
        reply(request, json!({"turn": {"id": turn}}))?;
        send(
            json!({"method": "turn/started", "params": {"threadId": self.thread, "turn": {"id": turn}}}),
        )?;
        if request["params"]["input"][0]["text"] != "wait" {
            send(
                json!({"method": "item/completed", "params": {"threadId": self.thread, "turnId": turn, "item": {"type": "commandExecution", "command": "fixture-check", "exitCode": 0}}}),
            )?;
            send(
                json!({"method": "item/completed", "params": {"threadId": self.thread, "turnId": turn, "item": {"type": "agentMessage", "phase": "final_answer", "text": "fixture complete"}}}),
            )?;
            self.complete(&json!(turn), "completed")?;
        }
        Ok(())
    }

    fn complete(&self, turn: &Value, status: &str) -> anyhow::Result<()> {
        send(
            json!({"method": "turn/completed", "params": {"threadId": self.thread, "turn": {"id": turn, "status": status}}}),
        )
    }

    fn approval(&mut self, request: &Value) -> anyhow::Result<()> {
        self.approval_request = Some(request.clone());
        send(json!({"id": 900, "method": "item/commandExecution/requestApproval", "params": {}}))
    }

    fn approval_response(&mut self, request: &Value) -> anyhow::Result<()> {
        let original = self
            .approval_request
            .take()
            .ok_or_else(|| anyhow::anyhow!("no approval pending"))?;
        anyhow::ensure!(request["id"] == 900, "wrong approval response id");
        reply(&original, json!({"approvalError": request["error"]}))
    }
}
