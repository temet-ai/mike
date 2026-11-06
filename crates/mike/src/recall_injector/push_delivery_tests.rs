use uuid::Uuid;

use crate::proxy::remote_control::ReplyStreams;
use crate::proxy::remote_control::inject::PushDeps;
use crate::store::RequestStore;
use crate::store::models::RequestId;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;
use crate::store::retention::RetentionPolicy;

use super::{RecallInjectionRequest, RecallInjectorConfig, RecallOutcome, apply_recall_injection};

const CSE: &str = "cse_recall_push_test";
const PROJECT_PATH: &str = "/tmp/recall-push-test-project";

async fn seed(handle: &PersistHandle) -> anyhow::Result<Uuid> {
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen) \
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind(PROJECT_PATH)
    .fetch_one(handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, topic)
         SELECT $1, lk.id, $2, ls.id, lsrc.id, $3
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
         WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'operator'",
    )
    .bind(project_id)
    .bind("never approve without running tests")
    .bind("code-review")
    .execute(handle.pool())
    .await?;
    let session = Uuid::new_v4();
    sqlx::query("INSERT INTO sessions.sessions(session_uuid, project_id) VALUES ($1, $2)")
        .bind(session)
        .bind(project_id)
        .execute(handle.pool())
        .await?;
    sqlx::query("INSERT INTO remote_control.channels(cse_id, local_session_uuid) VALUES ($1,$2)")
        .bind(CSE)
        .bind(session)
        .execute(handle.pool())
        .await?;
    Ok(session)
}

#[tokio::test]
async fn apply_recall_injection_appends_instead_of_pushing_to_a_live_channel() -> anyhow::Result<()>
{
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = seed(&handle).await?;
    let streams = ReplyStreams::default();
    let (_registration, mut rx) = streams.register(CSE.to_string());
    tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let _ = frame.result.send(true);
        }
    });
    let push_deps = PushDeps {
        persist: Some(handle.clone()),
        remote_control_streams: streams,
        gate: std::sync::Arc::new(crate::injection::InjectionGate::default()),
        store: std::sync::Arc::new(tokio::sync::RwLock::new(RequestStore::new(
            RetentionPolicy::default(),
        ))),
    };

    let raw = serde_json::to_vec(&serde_json::json!({
        "messages": [
            {"role": "assistant", "content": "<mem:recall topic=\"code-review\"/>"},
            {"role": "user", "content": [{"type": "text", "text": "go on"}]}
        ]
    }))?;
    let cfg = RecallInjectorConfig {
        enabled: true,
        max_lessons_per_topic: 5,
    };
    let outcome = apply_recall_injection(RecallInjectionRequest {
        raw: &raw,
        project_path: PROJECT_PATH,
        persist: &Some(handle.clone()),
        cfg: &cfg,
        request_id: RequestId::next(),
        session_id: &session.to_string(),
        push_deps: &push_deps,
    })
    .await;

    assert!(
        matches!(outcome, RecallOutcome::Injected { .. }),
        "recalled lessons ride the request body as a real <system-reminder>, \
         never a peer-framed client_event push: {outcome:?}"
    );
    let pushed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM remote_control.injections i \
         JOIN remote_control.channels c ON c.id = i.channel_id WHERE c.cse_id = $1",
    )
    .bind(CSE)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(
        pushed, 0,
        "a live channel was registered, so a zero count proves the policy declined the wake"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
