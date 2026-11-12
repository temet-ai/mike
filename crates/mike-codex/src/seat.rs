use std::time::Duration;

use super::events::{self, EventReceiver};
use serde_json::{Value, json};

use super::CodexError;
use super::model::listed;
use super::options::SeatOptions;
use super::rpc::AppServer;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurnOutcome {
    pub turn_id: String,
    pub status: String,
    pub final_text: String,
    pub commands: Vec<(String, Option<i64>)>,
    pub commands_truncated: bool,
    pub observations_incomplete: bool,
}

pub struct CodexSeat {
    server: AppServer,
    events: EventReceiver,
    options: SeatOptions,
    thread_id: String,
    forked_from: Option<String>,
}

impl CodexSeat {
    pub async fn launch(options: SeatOptions) -> Result<Self, CodexError> {
        let (server, events) = spawn(&options).await?;
        let answer = server.call("thread/start", options.thread_start()).await?;
        let thread_id = thread_id("thread/start", &answer)?;
        Ok(Self {
            server,
            events,
            options,
            thread_id,
            forked_from: None,
        })
    }

    pub async fn resume(options: SeatOptions, thread_id: &str) -> Result<Self, CodexError> {
        let (server, events) = spawn(&options).await?;
        let answer = server
            .call("thread/resume", options.thread_resume(thread_id))
            .await?;
        let resumed = self::thread_id("thread/resume", &answer)?;
        Ok(Self {
            server,
            events,
            options,
            thread_id: resumed,
            forked_from: None,
        })
    }

    pub async fn fork(options: SeatOptions, from_thread: &str) -> Result<Self, CodexError> {
        let (server, events) = spawn(&options).await?;
        let answer = server
            .call("thread/fork", options.thread_fork(from_thread))
            .await?;
        let thread_id = self::thread_id("thread/fork", &answer)?;
        Ok(Self {
            server,
            events,
            options,
            thread_id,
            forked_from: Some(from_thread.to_string()),
        })
    }

    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }

    pub fn forked_from(&self) -> Option<&str> {
        self.forked_from.as_deref()
    }

    pub fn pid(&self) -> Option<u32> {
        self.server.pid()
    }

    pub async fn model_is_listed(&self, model: &str) -> Result<bool, CodexError> {
        let answer = self.server.call("model/list", json!({})).await?;
        Ok(listed(&answer, model))
    }

    pub async fn brief(&self, text: &str) -> Result<String, CodexError> {
        let answer = self
            .server
            .call("turn/start", self.options.turn_start(&self.thread_id, text))
            .await?;
        answer["turn"]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| shape("turn/start", "turn.id", &answer))
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, CodexError> {
        self.server.call(method, params).await
    }

    pub async fn interrupt(&self, turn_id: &str) -> Result<(), CodexError> {
        self.server
            .call(
                "turn/interrupt",
                json!({"threadId": self.thread_id, "turnId": turn_id}),
            )
            .await
            .map(|_| ())
    }

    pub async fn next_event(&mut self) -> Option<Value> {
        self.events.recv().await
    }

    pub async fn wait_turn(
        &mut self,
        turn_id: &str,
        deadline: Duration,
    ) -> Result<TurnOutcome, CodexError> {
        let mut outcome = TurnOutcome {
            turn_id: turn_id.to_string(),
            ..TurnOutcome::default()
        };
        let waited = tokio::time::timeout(deadline, async {
            while let Some(event) = self.events.recv().await {
                if absorb(&mut outcome, &event, &self.thread_id) {
                    return Ok(());
                }
            }
            Err(CodexError::Closed("stdout"))
        })
        .await;
        match waited {
            Ok(result) => result.map(|()| outcome),
            Err(_) => Err(CodexError::TurnDeadline {
                thread_id: self.thread_id.clone(),
                turn_id: turn_id.to_string(),
            }),
        }
    }

    pub async fn stop(self) -> Result<(), CodexError> {
        self.server.stop().await
    }
}

async fn spawn(options: &SeatOptions) -> Result<(AppServer, EventReceiver), CodexError> {
    let (tx, rx) = events::channel();
    let server = AppServer::spawn(&options.server(), tx).await?;
    Ok((server, rx))
}

fn thread_id(method: &str, answer: &Value) -> Result<String, CodexError> {
    answer["thread"]["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| shape(method, "thread.id", answer))
}

fn shape(method: &str, field: &'static str, answer: &Value) -> CodexError {
    CodexError::Shape {
        method: method.to_string(),
        field,
        answer: answer.to_string(),
    }
}

pub(crate) fn absorb(outcome: &mut TurnOutcome, event: &Value, thread: &str) -> bool {
    let params = &event["params"];
    if event["method"] == "mike/observationGap" {
        outcome.observations_incomplete = true;
        return false;
    }
    if params["threadId"].as_str().is_some_and(|t| t != thread) {
        return false;
    }
    if params["turnId"]
        .as_str()
        .is_some_and(|turn| turn != outcome.turn_id)
    {
        return false;
    }
    match event["method"].as_str() {
        Some("item/completed") => {
            item(outcome, &params["item"]);
            false
        }
        Some("turn/completed") if params["turn"]["id"].as_str() == Some(&outcome.turn_id) => {
            outcome.status = params["turn"]["status"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            true
        }
        _ => false,
    }
}

fn item(outcome: &mut TurnOutcome, item: &Value) {
    match item["type"].as_str() {
        Some("agentMessage") if item["phase"].as_str() == Some("final_answer") => {
            outcome.final_text = item["text"].as_str().unwrap_or_default().to_string();
        }
        Some("commandExecution") => command(outcome, item),
        _ => {}
    }
}

fn command(outcome: &mut TurnOutcome, item: &Value) {
    let command = item["command"].as_str().unwrap_or_default();
    let bytes: usize = outcome.commands.iter().map(|(text, _)| text.len()).sum();
    if outcome.commands.len() >= 256 || bytes.saturating_add(command.len()) > 64 * 1024 {
        outcome.commands_truncated = true;
        return;
    }
    outcome
        .commands
        .push((command.to_owned(), item["exitCode"].as_i64()));
}
