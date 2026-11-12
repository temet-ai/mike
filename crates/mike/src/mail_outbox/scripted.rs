use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{Value, json};
use uuid::Uuid;

use super::codex::{CodexRecipient, SeatRpc};
use super::store::{self, NewMail};
use super::{Recipient, Runtime};
use crate::launch::codex::CodexError;
use crate::store::persistence::{PersistHandle, test_support};

#[derive(Debug, Default)]
pub(super) struct Script {
    pub(super) dead: bool,
    pub(super) generation: i64,
    pub(super) active: Option<String>,
    pub(super) turns: Vec<Value>,
    pub(super) lose_reply: bool,
    pub(super) fail_before_send: bool,
    pub(super) read_fails: bool,
    pub(super) end_turn_before_steer: bool,
    pub(super) calls: Vec<String>,
}

#[derive(Clone, Default)]
pub(super) struct ScriptedSeat(pub(super) Arc<Mutex<Script>>);

impl ScriptedSeat {
    pub(super) fn with(script: Script) -> Self {
        Self(Arc::new(Mutex::new(script)))
    }

    pub(super) fn edit(&self, change: impl FnOnce(&mut Script)) {
        if let Ok(mut script) = self.0.lock() {
            change(&mut script);
        }
    }

    pub(super) fn read<T>(&self, look: impl FnOnce(&Script) -> T) -> Option<T> {
        self.0.lock().ok().map(|script| look(&script))
    }

    pub(super) fn copies_of(&self, message: Uuid) -> usize {
        let id = message.to_string();
        self.read(|s| {
            s.turns
                .iter()
                .flat_map(|t| t["items"].as_array().cloned().unwrap_or_default())
                .filter(|i| i["clientId"].as_str() == Some(&id))
                .count()
        })
        .unwrap_or_default()
    }

    pub(super) fn busy(&self) -> String {
        let turn = format!("turn-{}", Uuid::new_v4());
        self.edit(|s| {
            s.active = Some(turn.clone());
            s.turns.push(json!({"id": turn, "status": "inProgress", "itemsView": "full",
                "items": [{"type": "userMessage", "id": "brief", "content": [{"type": "text", "text": "brief"}]},
                          {"type": "commandExecution", "id": "cmd-1"}]}));
        });
        turn
    }

    pub(super) fn recipient(&self) -> CodexRecipient<Self> {
        CodexRecipient {
            rpc: self.clone(),
            thread_id: "thread-1".into(),
            pool: None,
        }
    }
}

fn user_item(params: &Value) -> Value {
    json!({"type": "userMessage", "id": format!("item-{}", Uuid::new_v4()),
           "clientId": params["clientUserMessageId"], "content": params["input"]})
}

fn timeout(method: &str) -> CodexError {
    CodexError::Timeout(method.to_string())
}

fn refused(method: &str, error: &str) -> CodexError {
    CodexError::Rpc {
        method: method.to_string(),
        params: String::new(),
        error: error.to_string(),
    }
}

fn start(s: &mut Script, params: &Value) -> Result<Value, CodexError> {
    if s.active.is_some() {
        return Err(refused("turn/start", "a turn is already in progress"));
    }
    let turn = format!("turn-{}", Uuid::new_v4());
    s.turns.push(
        json!({"id": turn, "status": "completed", "itemsView": "full",
        "items": [user_item(params), {"type": "agentMessage", "id": format!("answer-{turn}")}]}),
    );
    if s.lose_reply {
        s.lose_reply = false;
        return Err(timeout("turn/start"));
    }
    Ok(json!({"turn": {"id": turn}}))
}

fn steer(s: &mut Script, params: &Value) -> Result<Value, CodexError> {
    if s.end_turn_before_steer {
        s.end_turn_before_steer = false;
        s.active = None;
        if let Some(last) = s.turns.last_mut() {
            last["status"] = json!("completed");
        }
    }
    let expected = params["expectedTurnId"].as_str().unwrap_or_default();
    if s.active.as_deref() != Some(expected) {
        return Err(refused(
            "turn/steer",
            "expectedTurnId does not match the active turn",
        ));
    }
    let item = user_item(params);
    if let Some(turn) = s.turns.iter_mut().find(|t| t["id"] == expected)
        && let Some(items) = turn["items"].as_array_mut()
    {
        items.push(item);
    }
    if s.lose_reply {
        s.lose_reply = false;
        return Err(timeout("turn/steer"));
    }
    Ok(json!({"turnId": expected}))
}

fn read(s: &Script) -> Result<Value, CodexError> {
    if s.read_fails {
        return Err(timeout("thread/read"));
    }
    let status = if s.active.is_some() { "active" } else { "idle" };
    Ok(json!({"thread": {"id": "thread-1", "status": {"type": status}, "turns": s.turns}}))
}

#[async_trait]
impl SeatRpc for ScriptedSeat {
    fn live(&self) -> bool {
        self.read(|s| !s.dead).unwrap_or(false)
    }

    async fn generation(&self) -> Option<i64> {
        self.read(|s| (!s.dead).then_some(s.generation)).flatten()
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, CodexError> {
        let mut s = self.0.lock().map_err(|_| CodexError::Closed("script"))?;
        s.calls.push(method.to_string());
        if s.fail_before_send && method != "thread/read" {
            s.fail_before_send = false;
            return Err(CodexError::Closed("stdin"));
        }
        match method {
            "thread/read" => read(&s),
            "turn/start" => start(&mut s, &params),
            "turn/steer" => steer(&mut s, &params),
            other => Err(refused(other, "not scripted")),
        }
    }
}

pub(super) async fn database() -> anyhow::Result<Option<(test_support::TestDbHandle, PersistHandle)>>
{
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    Ok(Some((db, persist)))
}

pub(super) async fn queued(persist: &PersistHandle, generation: i64) -> anyhow::Result<Uuid> {
    let message = Uuid::new_v4();
    let inserted = store::enqueue(
        persist.pool(),
        &NewMail {
            message,
            to: Uuid::new_v4(),
            from: Some(Uuid::new_v4()),
            runtime: Runtime::Codex,
            scope: Some("task X5 files only"),
            body: "please run the check",
            generation,
        },
    )
    .await?;
    anyhow::ensure!(inserted, "message {message} was not queued");
    Ok(message)
}

pub(super) fn as_recipient(seat: &CodexRecipient<ScriptedSeat>) -> &dyn Recipient {
    seat
}
