use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::runtime::{ObservationSource, SessionControl, SessionLifecycle, SessionObservation};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDb, TestDbHandle};
use crate::store::persistence::workspace::RegisterWorkspaceSession;

pub struct Fixture {
    pub db: TestDbHandle,
    pub handle: PersistHandle,
    pub session: Uuid,
    pub session_db: i64,
    pub project: i64,
}

pub async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = Uuid::new_v4();
    let path = format!("/tmp/observation-{}", session);
    let registered = handle
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid: session,
            project_path: Some(&path),
            cwd: &path,
            worktree: None,
            registered_via: "hook",
            intent: None,
            role: None,
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;
    let session_db = sqlx::query_scalar!(
        "SELECT id FROM sessions.sessions WHERE session_uuid = $1",
        session
    )
    .fetch_one(handle.pool())
    .await?;
    Ok(Some(Fixture {
        db,
        handle,
        session,
        session_db,
        project: registered.project_id,
    }))
}

pub fn observation(session: Uuid) -> anyhow::Result<SessionObservation> {
    let observed_at = DateTime::from_timestamp_micros(Utc::now().timestamp_micros())
        .ok_or_else(|| anyhow::anyhow!("clock out of range"))?;
    Ok(SessionObservation {
        session_id: session,
        observation_id: Uuid::new_v4(),
        runtime: "third-fixture".into(),
        source: ObservationSource {
            kind: "fixture".into(),
            version: Some("1".into()),
            fidelity: "provider_metadata".into(),
            completeness: "observed".into(),
        },
        observed_at,
        sequence: 1,
        lifecycle: Some(SessionLifecycle::Active),
        requested_model: Some("requested-model".into()),
        observed_model: Some("actual-model".into()),
        usage: None,
        model_call: None,
        item: None,
        control: Some(SessionControl::Managed),
    })
}

impl Fixture {
    pub async fn cleanup(self) -> anyhow::Result<()> {
        self.handle.pool().close().await;
        self.db.cleanup().await
    }
}
