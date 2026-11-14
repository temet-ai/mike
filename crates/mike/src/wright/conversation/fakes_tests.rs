use crate::store::persistence::web_repo::WebPersist as _;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::http::{HeaderMap, HeaderValue, header};
use serde_json::{Value, json};

use super::token::OperatorToken;
use super::{
    AnswerEngine, AnswerRequest, Change, Conversation, EngineError, SteerApply, SteerError,
};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use crate::store::persistence::wright::conversation::{NewOperatorMessage, Posted};

pub(super) const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

pub(super) fn operator_token() -> anyhow::Result<OperatorToken> {
    OperatorToken::from_text(TOKEN).ok_or_else(|| anyhow::anyhow!("the test token is empty"))
}

pub(super) fn bearer(token: &str) -> anyhow::Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))?,
    );
    Ok(headers)
}

#[derive(Default)]
pub(super) struct FakeEngine {
    answers: Mutex<VecDeque<Result<Value, EngineError>>>,
    pub(super) calls: AtomicUsize,
    pub(super) requests: Mutex<Vec<AnswerRequest>>,
}

impl FakeEngine {
    pub(super) fn answering(answers: Vec<Result<Value, EngineError>>) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.into()),
            ..Self::default()
        })
    }

    pub(super) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AnswerEngine for FakeEngine {
    async fn answer(&self, request: AnswerRequest) -> Result<Value, EngineError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut seen) = self.requests.lock() {
            seen.push(request);
        }
        let next = self
            .answers
            .lock()
            .ok()
            .and_then(|mut queue| queue.pop_front());
        next.unwrap_or_else(|| {
            Err(EngineError::Failed(
                "the fake engine has no answer queued".to_owned(),
            ))
        })
    }
}

#[derive(Default)]
pub(super) struct FakeSteers {
    resources: Mutex<HashMap<String, Value>>,
}

fn key(change: &Change) -> Option<String> {
    match change {
        Change::PauseRemedy { name, .. } | Change::ResumeRemedy { name } => {
            Some(format!("remedy:{name}"))
        }
        _ => None,
    }
}

impl FakeSteers {
    pub(super) fn state(&self, key: &str) -> Value {
        self.resources
            .lock()
            .ok()
            .and_then(|map| map.get(key).cloned())
            .unwrap_or_else(|| json!({"paused": false}))
    }

    pub(super) fn touch(&self, key: &str, value: Value) {
        if let Ok(mut map) = self.resources.lock() {
            map.insert(key.to_owned(), value);
        }
    }
}

#[async_trait]
impl SteerApply for FakeSteers {
    async fn read(&self, change: &Change) -> Result<Value, SteerError> {
        let key = key(change)
            .ok_or_else(|| SteerError::Refused("the fake knows only remedies".to_owned()))?;
        Ok(self.state(&key))
    }

    async fn apply(&self, steer_id: i64, change: &Change) -> Result<Value, SteerError> {
        let key = key(change)
            .ok_or_else(|| SteerError::Refused("the fake knows only remedies".to_owned()))?;
        let effect = match change {
            Change::PauseRemedy { until, .. } => {
                json!({"paused": true, "until": until, "steer": steer_id})
            }
            _ => json!({"paused": false}),
        };
        self.touch(&key, effect.clone());
        Ok(effect)
    }

    async fn undo(
        &self,
        change: &Change,
        prior: &Value,
        effect: &Value,
    ) -> Result<Value, SteerError> {
        let key = key(change)
            .ok_or_else(|| SteerError::Refused("the fake knows only remedies".to_owned()))?;
        let current = self.state(&key);
        if &current != effect {
            return Err(SteerError::Conflict(format!(
                "{key} is now {current}, not {effect}"
            )));
        }
        self.touch(&key, prior.clone());
        Ok(prior.clone())
    }
}

pub(super) struct Db {
    pub(super) db: TestDbHandle,
    pub(super) handle: PersistHandle,
}

impl Db {
    pub(super) async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        Ok(Self { db, handle })
    }

    pub(super) fn convo(
        &self,
        engine: Arc<dyn AnswerEngine>,
        steers: Arc<dyn SteerApply>,
    ) -> Conversation {
        Conversation {
            persist: self.handle.clone(),
            engine,
            steers,
        }
    }

    pub(super) async fn ask(&self, client_msg_id: &str, body: &str) -> anyhow::Result<i64> {
        let posted = self
            .handle
            .web()
            .post_wright_message(&NewOperatorMessage {
                client_msg_id,
                body,
                reply_to: None,
            })
            .await?;
        match posted {
            Posted::Created(row) => Ok(row.id),
            Posted::Existing(row) => anyhow::bail!("message {} already existed", row.id),
        }
    }

    pub(super) async fn replies_to(&self, message_id: i64) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT count(*) FROM wright.messages WHERE author = 'wright' AND reply_to = $1",
        )
        .bind(message_id)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub(super) async fn job_of(&self, message_id: i64) -> anyhow::Result<(String, i32)> {
        Ok(
            sqlx::query_as("SELECT state, attempts FROM wright.reply_jobs WHERE message_id = $1")
                .bind(message_id)
                .fetch_one(self.handle.pool())
                .await?,
        )
    }

    pub(super) async fn cleanup(self) -> anyhow::Result<()> {
        self.db.cleanup().await
    }
}
