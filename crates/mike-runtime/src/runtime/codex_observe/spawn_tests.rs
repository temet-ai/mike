use serde_json::json;
use uuid::Uuid;

use super::ingest::{self, ObserveSource};
use crate::runtime::{ProviderThreadRef, codex_home};
use crate::store::persistence::runtime_repo::RuntimePersist as _;
use crate::store::persistence::{PersistHandle, test_support};

struct Fixture {
    db: test_support::TestDbHandle,
    persist: PersistHandle,
    home: tempfile::TempDir,
    source: ObserveSource,
    session: Uuid,
}

impl Fixture {
    async fn new() -> anyhow::Result<Self> {
        let db = test_support::acquire_ephemeral_db().await?.ok_or_else(|| {
            anyhow::anyhow!("Codex spawn identity check requires the test database")
        })?;
        let persist = PersistHandle::connect(&db.database_url).await?;
        let home = tempfile::tempdir()?;
        let path = home.path().canonicalize()?.to_string_lossy().into_owned();
        let namespace = codex_home::namespace_for(std::path::Path::new(&path));
        persist
            .runtime()
            .register_runtime_namespace(&namespace)
            .await?;
        let session = Uuid::new_v4();
        sqlx::query("INSERT INTO launch.launches (session_uuid, backend, folder, identity) VALUES ($1, 'herdr', $2, $3)")
            .bind(session).bind(&path)
            .bind(json!({"runtime":"codex","runtime_home":path,"name":"spawned-codex"}))
            .execute(persist.pool()).await?;
        let source = ObserveSource {
            label: "spawn-fixture".into(),
            hook_boundary: true,
            codex_home: Some(path),
            spawn_session: Some(session),
            ..Default::default()
        };
        Ok(Self {
            db,
            persist,
            home,
            source,
            session,
        })
    }

    fn hook(&self, thread: Uuid) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec(
            &json!({"session_id":thread,"hook_event_name":"SessionStart",
            "cwd":self.home.path().canonicalize()?,"model":"spawn-model"}),
        )?)
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.persist.pool().close().await;
        self.db.cleanup().await
    }
}

#[tokio::test]
async fn spawned_hook_binds_native_thread_without_a_second_session_and_keeps_children_distinct()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let native = Uuid::new_v4();
    let pending =
        ingest::ingest_otel(&f.persist, &f.source, "logs", br#"{"resourceLogs":[]}"#).await?;
    assert_eq!(pending.availability, "awaiting_spawn_hook");
    let child = Uuid::new_v4();
    let child_hook = serde_json::to_vec(&json!({"session_id":native,"agent_id":child,
        "hook_event_name":"SubagentStart","cwd":f.source.codex_home}))?;
    let early_child =
        ingest::ingest_hook(&f.persist, &f.source, Uuid::new_v4(), &child_hook).await?;
    assert_eq!(early_child.availability, "awaiting_spawn_hook");
    assert_eq!(early_child.facts, 0);
    assert!(early_child.mike_session_id.is_none());
    let report =
        ingest::ingest_hook(&f.persist, &f.source, Uuid::new_v4(), &f.hook(native)?).await?;
    assert_eq!(report.mike_session_id, Some(f.session));
    let binding = f
        .persist
        .runtime()
        .runtime_thread(&ProviderThreadRef {
            namespace_id: report.namespace_id,
            thread_id: native.to_string(),
        })
        .await?
        .ok_or_else(|| anyhow::anyhow!("native thread was not bound"))?;
    assert_eq!(binding.mike_session_id, f.session);
    let duplicate: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions.sessions WHERE session_uuid=$1)")
            .bind(native)
            .fetch_one(f.persist.pool())
            .await?;
    assert!(!duplicate);
    let name: Option<String> =
        sqlx::query_scalar("SELECT g.display_name FROM sessions.session_goals g JOIN sessions.sessions s ON s.id=g.session_id WHERE s.session_uuid=$1")
            .bind(f.session)
            .fetch_one(f.persist.pool())
            .await?;
    assert_eq!(name.as_deref(), Some("spawned-codex"));
    let ready =
        ingest::ingest_otel(&f.persist, &f.source, "logs", br#"{"resourceLogs":[]}"#).await?;
    assert_eq!(ready.availability, "metadata_only");
    let child_report =
        ingest::ingest_hook(&f.persist, &f.source, Uuid::new_v4(), &child_hook).await?;
    assert_eq!(child_report.mike_session_id, Some(child));
    assert_ne!(child_report.mike_session_id, Some(f.session));
    f.cleanup().await
}

#[tokio::test]
async fn spawn_hint_cannot_adopt_an_unknown_closed_or_different_home_launch() -> anyhow::Result<()>
{
    let f = Fixture::new().await?;
    let body = f.hook(Uuid::new_v4())?;
    let unknown = ObserveSource {
        spawn_session: Some(Uuid::new_v4()),
        ..f.source.clone()
    };
    assert!(
        ingest::ingest_hook(&f.persist, &unknown, Uuid::new_v4(), &body)
            .await
            .is_err()
    );
    sqlx::query("UPDATE launch.launches SET identity = jsonb_set(identity, '{runtime_home}', '\"/other/home\"') WHERE session_uuid=$1")
        .bind(f.session).execute(f.persist.pool()).await?;
    assert!(
        ingest::ingest_hook(&f.persist, &f.source, Uuid::new_v4(), &body)
            .await
            .is_err()
    );
    sqlx::query("UPDATE launch.launches SET closed_at=now() WHERE session_uuid=$1")
        .bind(f.session)
        .execute(f.persist.pool())
        .await?;
    assert!(
        ingest::ingest_hook(&f.persist, &f.source, Uuid::new_v4(), &body)
            .await
            .is_err()
    );
    assert!(
        f.persist
            .runtime()
            .runtime_thread_by_session(f.session)
            .await?
            .is_none()
    );
    f.cleanup().await
}
