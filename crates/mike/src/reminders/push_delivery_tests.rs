use uuid::Uuid;

use crate::extract::request::TurnOrigin;
use crate::lesson_worker::LessonSnapshot;
use crate::proxy::remote_control::ReplyStreams;
use crate::proxy::remote_control::inject::PushDeps;
use crate::store::RequestStore;
use crate::store::models::RequestId;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;
use crate::store::retention::RetentionPolicy;

use super::agent_lessons::AgentLessonsRequest;
use super::session_lessons::SessionLessonsRequest;
use super::test_support::{code_reviewer_snapshot, mk_finding, sample_request, user_turn_body};
use super::{RemindersConfig, RemindersOutcome, apply_agent_lessons, apply_reminders};

const CSE: &str = "cse_reminders_push_test";

async fn seed_session_with_channel(handle: &PersistHandle) -> anyhow::Result<Uuid> {
    let session = Uuid::new_v4();
    sqlx::query(
        "WITH p AS (INSERT INTO projects.projects(project_path, display_name) VALUES ($1, $1) RETURNING id) \
         INSERT INTO sessions.sessions(session_uuid, project_id) SELECT $2, id FROM p",
    )
    .bind(format!("/tmp/reminders-push-test-{session}"))
    .bind(session)
    .execute(handle.pool())
    .await?;
    sqlx::query("INSERT INTO remote_control.channels(cse_id, local_session_uuid) VALUES ($1,$2)")
        .bind(CSE)
        .bind(session)
        .execute(handle.pool())
        .await?;
    Ok(session)
}

fn deps(persist: PersistHandle, streams: ReplyStreams) -> PushDeps {
    PushDeps {
        persist: Some(persist),
        remote_control_streams: streams,
        gate: std::sync::Arc::new(crate::injection::InjectionGate::default()),
        store: std::sync::Arc::new(tokio::sync::RwLock::new(RequestStore::new(
            RetentionPolicy::default(),
        ))),
    }
}

#[tokio::test]
async fn apply_reminders_appends_instead_of_pushing_to_a_live_channel() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = seed_session_with_channel(&handle).await?;
    let streams = ReplyStreams::default();
    let (_registration, mut rx) = streams.register(CSE.to_string());
    tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let _ = frame.result.send(true);
        }
    });
    let push_deps = deps(handle.clone(), streams);

    let store = tokio::sync::RwLock::new(RequestStore::new(RetentionPolicy::default()));
    {
        let mut guard = store.write().await;
        let mut req = sample_request(RequestId::from_raw(1), &session.to_string());
        req.diagnostics = vec![mk_finding("comprehensive solution", "no MVP", 0)];
        guard.insert(req);
    }
    let cfg = RemindersConfig {
        enabled: true,
        ..Default::default()
    };
    let body = user_turn_body()?;
    let outcome = apply_reminders(
        &body,
        &session.to_string(),
        &store,
        &cfg,
        RequestId::next(),
        &push_deps,
    )
    .await;

    assert!(
        matches!(outcome, RemindersOutcome::Appended { .. }),
        "proxy-authored context rides the request body as a real <system-reminder>, \
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

#[tokio::test]
async fn apply_agent_lessons_appends_instead_of_pushing_to_a_live_channel() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = seed_session_with_channel(&handle).await?;
    let streams = ReplyStreams::default();
    let (_registration, mut rx) = streams.register(CSE.to_string());
    tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let _ = frame.result.send(true);
        }
    });
    let push_deps = deps(handle.clone(), streams);

    let body = crate::reminders::test_support::subagent_body_with_system()?;
    let snap = code_reviewer_snapshot();
    let outcome = apply_agent_lessons(AgentLessonsRequest {
        body_bytes: &body,
        session_id: &session.to_string(),
        subagent_persona: Some("code-reviewer"),
        turn_origin: &TurnOrigin::Subagent,
        snapshot: &snap,
        agent_lessons_enabled: true,
        request_id: RequestId::next(),
        push_deps: &push_deps,
    })
    .await;

    assert!(
        matches!(outcome, RemindersOutcome::Appended { .. }),
        "proxy-authored context rides the request body as a real <system-reminder>, \
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

#[tokio::test]
async fn apply_session_lessons_appends_instead_of_pushing_to_a_live_channel() -> anyhow::Result<()>
{
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = seed_session_with_channel(&handle).await?;
    let streams = ReplyStreams::default();
    let (_registration, mut rx) = streams.register(CSE.to_string());
    tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let _ = frame.result.send(true);
        }
    });
    let push_deps = deps(handle.clone(), streams);

    let cfg = RemindersConfig {
        inject_lessons: true,
        max_lessons_to_inject: 10,
        ..Default::default()
    };
    let body = serde_json::to_vec(
        &serde_json::json!({"messages":[{"role":"user","content":[{"type":"text","text":"Run make targets"}]}]}),
    )?;
    let snap = LessonSnapshot::from_pairs([(
        "my-project".to_string(),
        vec![(
            "correction".to_string(),
            "always use make targets".to_string(),
        )],
    )]);
    let outcome = super::apply_session_lessons(SessionLessonsRequest {
        body_bytes: &body,
        session_id: &session.to_string(),
        project_path: "my-project",
        is_tool_loop: false,
        turn_origin: &TurnOrigin::PrimaryFirst,
        snapshot: &snap,
        cfg: &cfg,
        recall_injector_enabled: false,
        request_id: RequestId::next(),
        push_deps: &push_deps,
    })
    .await;

    assert!(
        matches!(outcome, RemindersOutcome::Appended { .. }),
        "proxy-authored context rides the request body as a real <system-reminder>, \
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
