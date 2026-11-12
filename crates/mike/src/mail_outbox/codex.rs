use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::envelope::marker;
use super::mailbox_link::mark_taken;
use super::{Delivery, Found, Mode, Reach, Recipient, Sent, Sighting, Stage};
use crate::launch::codex::{CodexError, call_seat, live_seat};
use crate::store::persistence::PersistHandle;

#[async_trait]
pub(crate) trait SeatRpc: Send + Sync {
    fn live(&self) -> bool;
    async fn generation(&self) -> Option<i64>;
    async fn call(&self, method: &str, params: Value) -> Result<Value, CodexError>;
}

pub(crate) struct LiveSeat {
    pub(crate) persist: PersistHandle,
    pub(crate) session: Uuid,
}

#[async_trait]
impl SeatRpc for LiveSeat {
    fn live(&self) -> bool {
        live_seat(self.session)
    }

    async fn generation(&self) -> Option<i64> {
        if !self.live() {
            return None;
        }
        match self.persist.find_launch_by_session(self.session).await {
            Ok(row) => row.map(|launch| launch.id),
            Err(error) => {
                tracing::warn!(error = %error, session_id = %self.session, "mail outbox could not read the seat's launch");
                None
            }
        }
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, CodexError> {
        call_seat(self.session, method, params).await
    }
}

pub(crate) struct CodexRecipient<R> {
    pub(crate) rpc: R,
    pub(crate) thread_id: String,
    pub(crate) pool: Option<PgPool>,
}

impl<R: SeatRpc> CodexRecipient<R> {
    async fn read_thread(&self) -> Result<Value, String> {
        self.rpc
            .call(
                "thread/read",
                json!({"threadId": self.thread_id, "includeTurns": true}),
            )
            .await
            .map(|answer| answer["thread"].clone())
            .map_err(|error| error.to_string())
    }
}

#[async_trait]
impl<R: SeatRpc> Recipient for CodexRecipient<R> {
    async fn generation(&self) -> Option<i64> {
        self.rpc.generation().await
    }

    async fn reach(&self) -> Reach {
        if !self.rpc.live() {
            return Reach::Unreachable("no Codex seat runs for this session in this mike".into());
        }
        match self.read_thread().await {
            Ok(thread) => reach_of(&thread),
            Err(error) => Reach::Unreachable(format!("thread/read failed: {error}")),
        }
    }

    async fn find(&self, message: Uuid) -> Found {
        if !self.rpc.live() {
            return Found::Unknown("no Codex seat runs for this session to read".into());
        }
        match self.read_thread().await {
            Ok(thread) => sighting_in(&thread, message),
            Err(error) => Found::Unknown(format!("thread/read failed: {error}")),
        }
    }

    async fn send(&self, delivery: &Delivery<'_>) -> Sent {
        let input = json!([{"type": "text", "text": delivery.text}]);
        let client_id = delivery.message.to_string();
        let (method, params) = match &delivery.mode {
            Mode::Start => (
                "turn/start",
                json!({"threadId": self.thread_id, "input": input, "clientUserMessageId": client_id}),
            ),
            Mode::Steer { expected_turn } => (
                "turn/steer",
                json!({"threadId": self.thread_id, "expectedTurnId": expected_turn, "input": input, "clientUserMessageId": client_id}),
            ),
        };
        match self.rpc.call(method, params).await {
            Ok(answer) => accepted(method, &answer),
            Err(error) => classify(&error),
        }
    }

    async fn taken(&self, message: Uuid, consumed: bool) {
        if let Some(pool) = &self.pool
            && let Err(error) = mark_taken(pool, message, consumed).await
        {
            tracing::warn!(error = %error, message_id = %message, "mail outbox could not mark the mailbox row taken");
        }
    }
}

fn accepted(method: &str, answer: &Value) -> Sent {
    let turn = answer["turn"]["id"]
        .as_str()
        .or_else(|| answer["turnId"].as_str());
    match turn {
        Some(turn_id) => Sent::Accepted {
            turn_id: turn_id.to_string(),
        },
        None => Sent::FailedAfterSend(format!("{method} answered without a turn id: {answer}")),
    }
}

pub(crate) fn classify(error: &CodexError) -> Sent {
    match error {
        CodexError::Rpc { error, .. } if error.to_lowercase().contains("turn") => {
            Sent::TurnEnded(error.clone())
        }
        CodexError::Rpc { method, error, .. } => Sent::Refused(format!("{method}: {error}")),
        CodexError::Closed("stdin" | "seat control") | CodexError::Spawn { .. } => {
            Sent::FailedBeforeSend(error.to_string())
        }
        _ => Sent::FailedAfterSend(error.to_string()),
    }
}

pub(crate) fn reach_of(thread: &Value) -> Reach {
    match thread["status"]["type"].as_str() {
        Some("idle") => Reach::Idle,
        Some("active") => turns(thread)
            .iter()
            .rev()
            .find(|turn| turn["status"] == "inProgress")
            .and_then(|turn| turn["id"].as_str())
            .map_or_else(
                || Reach::Unreachable("the thread is active but lists no turn in progress".into()),
                |turn_id| Reach::Busy {
                    turn_id: turn_id.to_string(),
                },
            ),
        other => Reach::Unreachable(format!(
            "the thread status is {}",
            other.unwrap_or("missing")
        )),
    }
}

fn turns(thread: &Value) -> &[Value] {
    thread["turns"].as_array().map_or(&[], Vec::as_slice)
}

fn holds(item: &Value, message: Uuid, mark: &str) -> bool {
    item["type"] == "userMessage"
        && (item["clientId"].as_str() == Some(&message.to_string())
            || item["content"].as_array().is_some_and(|parts| {
                parts
                    .iter()
                    .any(|p| p["text"].as_str().is_some_and(|t| t.contains(mark)))
            }))
}

pub(crate) fn sighting_in(thread: &Value, message: Uuid) -> Found {
    if thread["turns"].as_array().is_none() {
        return Found::Unknown("thread/read returned no turn list".into());
    }
    let mark = marker(message);
    for turn in turns(thread) {
        let items = turn["items"].as_array().map_or(&[][..], Vec::as_slice);
        if let Some(at) = items.iter().position(|item| holds(item, message, &mark)) {
            return Found::Seen(sighting(turn, items, at));
        }
    }
    let partial = turns(thread)
        .iter()
        .any(|turn| turn["itemsView"].as_str().is_some_and(|v| v != "full"));
    if partial {
        Found::Unknown("thread/read returned partial items for a turn".into())
    } else {
        Found::NotSeen
    }
}

fn sighting(turn: &Value, items: &[Value], at: usize) -> Sighting {
    let turn_id = turn["id"].as_str().unwrap_or("unknown").to_string();
    let item_id = items
        .get(at)
        .and_then(|item| item["id"].as_str())
        .unwrap_or("unknown");
    let held = format!("thread/read: turn {turn_id} holds the message as item {item_id}");
    let mut stages = vec![(Stage::Accepted, held.clone()), (Stage::Consumed, held)];
    let action = items
        .iter()
        .skip(at + 1)
        .find(|item| item["type"] != "userMessage");
    if let Some(action) = action {
        stages.push((
            Stage::ActionStarted,
            format!(
                "thread/read: item {} ({}) follows it in turn {turn_id}",
                action["id"].as_str().unwrap_or("unknown"),
                action["type"].as_str().unwrap_or("unknown"),
            ),
        ));
        if turn["status"] == "completed" {
            stages.push((
                Stage::Completed,
                format!("thread/read: turn {turn_id} completed"),
            ));
        }
    }
    Sighting {
        turn_id: Some(turn_id),
        stages,
    }
}
