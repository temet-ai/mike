use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::sync::atomic::{AtomicI32, Ordering};

use serde_json::{Value, json};
use uuid::Uuid;

use super::NewSearchEvent;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_project, seed_session_turns,
};

pub(crate) const SEARCH_TOOL: &str = "mcp__mike__search_project_knowledge";

pub(crate) struct Fixture {
    pub(crate) db: TestDbHandle,
    pub(crate) handle: PersistHandle,
    pub(crate) session: Uuid,
    pub(crate) requests: Vec<i64>,
    pub(crate) turns: Vec<i64>,
    next_ord: AtomicI32,
}

impl Fixture {
    pub(crate) async fn boot(turn_count: i64) -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        let project = seed_project(handle.pool(), "/search-events/project").await?;
        let session = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1, $2, now(), now())",
        )
        .bind(session)
        .bind(project)
        .execute(handle.pool())
        .await?;
        seed_session_turns(handle.pool(), &session.to_string(), project, turn_count).await?;
        let requests = request_ids(&handle).await?;
        let turns = turn_start_ids(&handle).await?;
        Ok(Self {
            db,
            handle,
            session,
            requests,
            turns,
            next_ord: AtomicI32::new(0),
        })
    }

    pub(crate) async fn tool_use(
        &self,
        request_index: usize,
        tool_name: &str,
        input: Value,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO tools.uses (wire_tool_use_id, request_id, session_id, ord, tool_name, input)
             SELECT 'toolu_' || gen_random_uuid(), r.id, r.session_id, $4, $2, $3
               FROM requests.requests r WHERE r.id = $1",
        )
        .bind(self.requests[request_index])
        .bind(tool_name)
        .bind(input)
        .bind(self.next_ord.fetch_add(1, Ordering::Relaxed))
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }

    pub(crate) async fn final_reply(&self, turn_index: usize, text: &str) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE memory.operator_turns SET final_assistant_text = $2
              WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $3)
                AND start_request_id = $1",
        )
        .bind(self.turns[turn_index])
        .bind(text)
        .bind(self.session)
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }

    pub(crate) async fn user_prompt(&self, turn_index: usize, text: &str) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE memory.operator_turns SET user_prompt = $2
              WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $3)
                AND start_request_id = $1",
        )
        .bind(self.turns[turn_index])
        .bind(text)
        .bind(self.session)
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }

    pub(crate) async fn record(
        &self,
        session: Option<Uuid>,
        query: &str,
        references: &[&str],
    ) -> anyhow::Result<i64> {
        let returned = Value::Array(
            references
                .iter()
                .map(|r| json!({"reference": r, "score": 0.5}))
                .collect(),
        );
        Ok(self
            .handle
            .knowledge()
            .record_search_event(&NewSearchEvent {
                session,
                tool: "search_project_knowledge",
                query,
                kind_filter: None,
                returned: &returned,
                rerank_state: "off",
            })
            .await?)
    }

    pub(crate) async fn record_returned(
        &self,
        tool: &str,
        query: &str,
        returned: &Value,
    ) -> anyhow::Result<i64> {
        Ok(self
            .handle
            .knowledge()
            .record_search_event(&NewSearchEvent {
                session: Some(self.session),
                tool,
                query,
                kind_filter: None,
                returned,
                rerank_state: "off",
            })
            .await?)
    }

    pub(crate) async fn event(&self, id: i64) -> anyhow::Result<(Option<i64>, Option<i64>)> {
        Ok(
            sqlx::query_as("SELECT session_id, request_id FROM memory.search_events WHERE id = $1")
                .bind(id)
                .fetch_one(self.handle.pool())
                .await?,
        )
    }

    pub(crate) async fn session_row(&self) -> anyhow::Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1")
                .bind(self.session)
                .fetch_one(self.handle.pool())
                .await?,
        )
    }

    pub(crate) async fn used(&self, id: i64) -> anyhow::Result<(Vec<String>, bool)> {
        Ok(sqlx::query_as(
            "SELECT used_references, used_final_at IS NOT NULL
               FROM memory.search_events WHERE id = $1",
        )
        .bind(id)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub(crate) async fn cleanup(self) -> anyhow::Result<()> {
        self.handle.pool().close().await;
        self.db.cleanup().await
    }
}

async fn request_ids(handle: &PersistHandle) -> anyhow::Result<Vec<i64>> {
    Ok(
        sqlx::query_scalar("SELECT id FROM requests.requests ORDER BY id")
            .fetch_all(handle.pool())
            .await?,
    )
}

async fn turn_start_ids(handle: &PersistHandle) -> anyhow::Result<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT start_request_id FROM memory.operator_turns ORDER BY start_request_id",
    )
    .fetch_all(handle.pool())
    .await?)
}
