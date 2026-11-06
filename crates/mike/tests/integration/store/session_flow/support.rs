use uuid::Uuid;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::id_resolvers::resolve_session_id;
use mike::store::persistence::session_flow::{FlowComponent, FlowCursor, FlowPage, FlowRowId};
use mike::store::persistence::workspace::RegisterWorkspaceSession;
use mike::store::test_support::{TestDb, TestDbHandle};

pub(crate) struct Fixture {
    pub(crate) db: TestDbHandle,
    pub(crate) handle: PersistHandle,
    pub(crate) session: Uuid,
    pub(crate) id: i64,
    pub(crate) project: i64,
}

impl Fixture {
    pub(crate) async fn new() -> anyhow::Result<Self> {
        let db = TestDb::bootstrap()
            .await?
            .ok_or_else(|| anyhow::anyhow!("session_flow tests require MIKE_TEST_ADMIN_URL"))?
            .acquire()
            .await?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session = Uuid::new_v4();
        resolve_session_id(handle.pool(), &session.to_string(), None, Some(1)).await?;
        let registration = handle
            .register_workspace_session(RegisterWorkspaceSession {
                session_uuid: session,
                project_path: None,
                cwd: "/tmp/flow-read",
                worktree: None,
                registered_via: "hook",
                intent: None,
                role: None,
                name: None,
                instructions: None,
                pid: None,
            })
            .await?;
        let id = sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session)
            .fetch_one(handle.pool())
            .await?;
        Ok(Self {
            db,
            handle,
            session,
            id,
            project: registration.project_id,
        })
    }

    pub(crate) async fn agent(&self, name: &str) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "INSERT INTO agents.agents (name) VALUES ($1)
             ON CONFLICT (LOWER(name)) DO UPDATE SET name = agents.agents.name RETURNING id",
        )
        .bind(name)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub(crate) async fn request(&self, worker: Option<&str>, origin: &str) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "INSERT INTO requests.requests
             (session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
              status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
              claude_code_agent_id, created_at, persisted_at, request_body)
             SELECT $1, $2, m.id, '/v1/messages', true, 1, false, s.id,
                    '{\"RAW_SENTINEL\":\"secret\"}'::jsonb, 'RAW_SENTINEL', 'RAW_SENTINEL', t.id, o.id,
                    $3, '2026-09-10T12:00:00Z'::timestamptz, '2026-09-10T12:00:01Z'::timestamptz, '\\x5241575f53454e54494e454c'::bytea
             FROM requests.methods m, requests.statuses s, requests.turn_types t, requests.turn_origins o
             WHERE m.code = 'POST' AND s.code = 'pending' AND t.code = 'user_prompt' AND o.code = $4 RETURNING id",
        ).bind(self.id).bind(self.project).bind(worker).bind(origin).fetch_one(self.handle.pool()).await?)
    }

    pub(crate) async fn page(
        &self,
        component: FlowComponent,
        limit: u16,
        cursor: Option<&FlowCursor>,
    ) -> anyhow::Result<FlowPage> {
        self.handle
            .fetch_session_flow(self.session, component, limit, cursor)
            .await?
            .ok_or_else(|| anyhow::anyhow!("fixture session missing"))
    }

    pub(crate) async fn cleanup(self) -> anyhow::Result<()> {
        self.handle.pool().close().await;
        self.db.cleanup().await
    }
}

pub(crate) fn ids(page: &FlowPage) -> Vec<i64> {
    let mut ids: Vec<_> = page
        .activities
        .iter()
        .filter_map(|a| match a.source.row_id {
            FlowRowId::Bigint(id) => Some(id),
            _ => None,
        })
        .collect();
    ids.dedup();
    ids
}

pub(crate) fn no_raw(page: &FlowPage) {
    assert!(!format!("{page:?}").contains("RAW_SENTINEL"));
    assert!(page.activities.iter().all(|a| a.source_revision.is_none()));
    assert!(
        page.actors
            .iter()
            .all(|a| a.parent_actor_id.is_none() && a.label.len() <= 128)
    );
}
