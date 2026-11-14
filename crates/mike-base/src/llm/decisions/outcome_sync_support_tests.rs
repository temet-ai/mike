use serde_json::{Value, json};
use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{
    TestDb, TestDbHandle, seed_project, seed_session_turns, seed_two_sessions,
};

pub(super) struct Env {
    pub db: TestDbHandle,
    pub handle: PersistHandle,
    pub project: i64,
    pub session_uuid: Uuid,
    pub session_id: i64,
}

pub(super) struct Turn {
    pub id: i64,
    pub start_request_id: i64,
}

pub(super) struct NewRecord<'a> {
    pub point: &'a str,
    pub applied: Option<&'a str>,
    pub composed: Value,
    pub session_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
    pub request_id: Option<i64>,
    pub project_id: Option<i64>,
    pub check_receipt_id: Option<i64>,
    pub commit_sha: Option<String>,
    pub message_uuid: Option<Uuid>,
    pub age_days: i32,
}

impl<'a> NewRecord<'a> {
    pub fn new(point: &'a str, applied: &'a str) -> Self {
        Self {
            point,
            applied: Some(applied),
            composed: json!({"verdict": {"score": 0.9}}),
            session_id: None,
            operator_turn_id: None,
            request_id: None,
            project_id: None,
            check_receipt_id: None,
            commit_sha: None,
            message_uuid: None,
            age_days: 0,
        }
    }

    pub fn verdict(mut self, verdict: Value) -> Self {
        self.composed = json!({ "verdict": verdict });
        self
    }
}

impl Env {
    pub async fn boot(turns: i64) -> anyhow::Result<Option<Self>> {
        let Some(factory) = TestDb::bootstrap().await? else {
            return Ok(None);
        };
        let db = factory.acquire_isolated().await?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        let project = seed_project(handle.pool(), &format!("/outcomes/{}", Uuid::new_v4())).await?;
        let session_uuid = Uuid::new_v4();
        seed_two_sessions(
            handle.pool(),
            &session_uuid.to_string(),
            &Uuid::new_v4().to_string(),
            project,
        )
        .await?;
        if turns > 0 {
            seed_session_turns(handle.pool(), &session_uuid.to_string(), project, turns).await?;
        }
        let session_id = sqlx::query_scalar!(
            "SELECT id FROM sessions.sessions WHERE session_uuid = $1",
            session_uuid
        )
        .fetch_one(handle.pool())
        .await?;
        Ok(Some(Self {
            db,
            handle,
            project,
            session_uuid,
            session_id,
        }))
    }

    pub async fn finish(self) -> anyhow::Result<()> {
        self.handle.pool().close().await;
        self.db.cleanup().await
    }

    pub async fn turns(&self) -> anyhow::Result<Vec<Turn>> {
        let rows = sqlx::query!(
            "SELECT id, start_request_id FROM memory.operator_turns
              WHERE session_id = $1 ORDER BY id",
            self.session_id
        )
        .fetch_all(self.handle.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Turn {
                id: r.id,
                start_request_id: r.start_request_id,
            })
            .collect())
    }

    pub async fn set_outcome(&self, turn_id: i64, code: &str, source: &str) -> anyhow::Result<()> {
        sqlx::query!(
            "UPDATE memory.operator_turns
                SET outcome_id = (SELECT id FROM memory.turn_outcomes WHERE code = $2),
                    outcome_source = $3
              WHERE id = $1",
            turn_id,
            code,
            source
        )
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }

    pub async fn record(&self, new: NewRecord<'_>) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar!(
            "INSERT INTO decisions.records
                 (created_at, point, mode, question_set, question_set_hash, model_requested,
                  state, questions, request_body, outcome, session_id, operator_turn_id,
                  request_id, project_id, check_receipt_id, composed, applied, commit_sha,
                  message_uuid)
             VALUES (now() - ($1::int * interval '1 day'), $2, 'on', 'test_v1', 'h',
                     'typesafe/jev-1.14', '{}'::jsonb, '{}'::jsonb, '{}'::jsonb, 'ok',
                     $3, $4, $5, $6, $7, $8, $9, $10, $11)
             RETURNING id",
            new.age_days,
            new.point,
            new.session_id,
            new.operator_turn_id,
            new.request_id,
            new.project_id,
            new.check_receipt_id,
            new.composed,
            new.applied,
            new.commit_sha,
            new.message_uuid,
        )
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub async fn labels(&self, record_id: i64) -> anyhow::Result<Vec<(String, String)>> {
        let rows = sqlx::query!(
            "SELECT label, label_source FROM decisions.outcomes
              WHERE record_id = $1 ORDER BY id",
            record_id
        )
        .fetch_all(self.handle.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.label, r.label_source))
            .collect())
    }

    pub async fn sync(&self) -> anyhow::Result<u64> {
        Ok(super::store(&self.handle)
            .sync_downstream_outcomes()
            .await?)
    }

    pub async fn lesson(&self) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar!(
            "INSERT INTO memory.lessons (kind_id, text, project_id)
             SELECT id, 'a lesson', $1 FROM memory.lesson_kinds ORDER BY id LIMIT 1
             RETURNING id",
            self.project
        )
        .fetch_one(self.handle.pool())
        .await?)
    }
}

pub(super) fn pair(label: &str, source: &str) -> (String, String) {
    (label.to_owned(), source.to_owned())
}
