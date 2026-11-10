use axum::Json;
use axum::extract::{Query, State};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::config::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

use super::*;

pub(super) struct Fixture {
    pub(super) state: AppState,
    pub(super) session_uuid: String,
    session_db_id: i64,
    project_id: i64,
    next_offset_ms: i64,
    next_sha: i64,
    db: Option<TestDbHandle>,
}

pub(super) async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_path = format!("/tmp/find-text-{}", Uuid::new_v4());
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen) \
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind(&project_path)
    .fetch_one(handle.pool())
    .await?;
    let session_uuid = Uuid::new_v4();
    let session_db_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .fetch_one(handle.pool())
    .await?;
    let mut state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    state.persist = Some(handle);
    Ok(Some(Fixture {
        state,
        session_uuid: session_uuid.to_string(),
        session_db_id,
        project_id,
        next_offset_ms: -60_000,
        next_sha: 0,
        db: Some(db),
    }))
}

impl Fixture {
    pub(super) fn pool(&self) -> anyhow::Result<&sqlx::PgPool> {
        let handle = self
            .state
            .persist()
            .ok_or_else(|| anyhow::anyhow!("fixture state has no persistence"))?;
        Ok(handle.pool())
    }

    pub(super) async fn finish(mut self) -> anyhow::Result<()> {
        if let Some(handle) = self.state.persist.take() {
            handle.pool().close().await;
        }
        if let Some(db) = self.db.take() {
            db.cleanup().await?;
        }
        Ok(())
    }

    pub(super) async fn add_request(&mut self, body: &str) -> anyhow::Result<i64> {
        self.next_offset_ms += 10;
        let created_at = chrono::Utc::now() + chrono::Duration::milliseconds(self.next_offset_ms);
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO requests.requests (
                session_id, project_id, method_id, path, is_streaming, message_count,
                is_tool_loop, status_id, params, last_user_message, human_prompt,
                turn_type_id, turn_origin_id, request_body, created_at
             )
             SELECT $1, $2, rm.id, '/v1/messages', false, 0, false, rs.id, '{}'::jsonb,
                    '', '', rtt.id, rto.id, $3, $4
             FROM requests.methods rm, requests.statuses rs,
                  requests.turn_types rtt, requests.turn_origins rto
             WHERE rm.code = 'POST' AND rs.code = 'complete'
               AND rtt.code = 'user_prompt' AND rto.code = 'primary'
             RETURNING id",
        )
        .bind(self.session_db_id)
        .bind(self.project_id)
        .bind(body.as_bytes())
        .bind(created_at)
        .fetch_one(self.pool()?)
        .await?;
        Ok(id)
    }

    pub(super) async fn add_message(
        &mut self,
        request_id: i64,
        ord: i32,
        body: &Value,
    ) -> anyhow::Result<i64> {
        self.next_sha += 1;
        let text = body.to_string();
        let role = body["role"].as_str().unwrap_or("user").to_owned();
        let message_id: i64 = sqlx::query_scalar(
            "INSERT INTO messages.messages (message_sha256, body, byte_size, role_id)
             SELECT $1, $2, $3, r.id FROM messages.roles r WHERE r.code = $4
             RETURNING id",
        )
        .bind(format!("{}-{}", self.session_uuid, self.next_sha))
        .bind(body)
        .bind(text.len() as i32)
        .bind(role)
        .fetch_one(self.pool()?)
        .await?;
        self.add_delta(request_id, ord, message_id).await?;
        Ok(message_id)
    }

    pub(super) async fn add_delta(
        &self,
        request_id: i64,
        ord: i32,
        message_id: i64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO requests.deltas (request_id, session_id, message_id, change_kind_id, ord)
             SELECT $1, $2, $3, k.id, $4 FROM requests.delta_change_kinds k WHERE k.code = 'added'",
        )
        .bind(request_id)
        .bind(self.session_db_id)
        .bind(message_id)
        .bind(ord)
        .execute(self.pool()?)
        .await?;
        Ok(())
    }

    pub(super) async fn add_response(&self, request_id: i64, body: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO responses.responses (
                request_id, response_status, response_headers, response_body,
                token_usage, stop_reason_id, completed_at
             )
             SELECT $1, 200, '[]'::jsonb, $2, '{}'::jsonb, sr.id, NOW()
             FROM responses.stop_reasons sr WHERE sr.code = 'end_turn'",
        )
        .bind(request_id)
        .bind(body.as_bytes())
        .execute(self.pool()?)
        .await?;
        Ok(())
    }

    pub(super) async fn add_tool_use(
        &self,
        request_id: i64,
        tool_name: &str,
        input: &Value,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
             VALUES ($1, $2, 0, $3, $4, $5)",
        )
        .bind(format!("toolu_{}", Uuid::new_v4().simple()))
        .bind(request_id)
        .bind(tool_name)
        .bind(input)
        .bind(self.session_db_id)
        .execute(self.pool()?)
        .await?;
        Ok(())
    }

    pub(super) async fn find_limited(
        &self,
        needle: &str,
        cursor: Option<u64>,
        limits: ScanLimits,
    ) -> anyhow::Result<Value> {
        let params = FindTextMatchParams {
            needle,
            source: FindTextSource::Request,
            case_sensitive: true,
            context_bytes: 20,
            max_matches: 50,
            whole_bodies: false,
            include_reminders: false,
        };
        match find_text_scoped_session(&self.state, &self.session_uuid, &params, cursor, &limits)
            .await
        {
            Ok(Json(response)) => Ok(serde_json::to_value(&response)?),
            Err((status, Json(body))) => {
                anyhow::bail!("find_text returned {status}: {}", body.error)
            }
        }
    }

    pub(super) async fn find(&self, query: Value) -> anyhow::Result<Value> {
        let mut query = query;
        query["session_id"] = json!(self.session_uuid);
        run_find(&self.state, query).await
    }
}

pub(super) async fn run_find(state: &AppState, query: Value) -> anyhow::Result<Value> {
    let params: FindTextUnifiedParams = serde_json::from_value(query)?;
    match find_text(State(state.clone()), Query(params)).await {
        Ok(Json(response)) => Ok(serde_json::to_value(&response)?),
        Err((status, Json(body))) => anyhow::bail!("find_text returned {status}: {}", body.error),
    }
}

pub(super) fn user_text(text: &str) -> Value {
    json!({"role": "user", "content": [{"type": "text", "text": text}]})
}

pub(super) fn match_request_ids(answer: &Value) -> Vec<u64> {
    answer["matches"]
        .as_array()
        .map(|matches| {
            matches
                .iter()
                .filter_map(|m| m["request_id"].as_u64())
                .collect()
        })
        .unwrap_or_default()
}
