use std::sync::Arc;

use serde_json::{Value, json};

use super::{
    RecallInjectionRequest, RecallInjectorConfig, RecallOutcome, apply_recall_injection_with,
};
use crate::llm::OpenRouterClient;
use crate::llm::decisions::env::DecisionEnv;
use crate::proxy::remote_control::ReplyStreams;
use crate::proxy::remote_control::inject::PushDeps;
use crate::reminders::supply_floor::rows_tests::{asked_bodies, reply, stub};
use crate::settings::SettingsOverlay;
use crate::store::models::RequestId;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::NewLessonWrite;
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use crate::store::persistence::test_support::{acquire_ephemeral_db, seed_project};

const FIRST: &str = "use the staging bucket for deploy checks";
const SECOND: &str = "tag the release before the deploy";

fn push_deps() -> PushDeps {
    PushDeps {
        persist: None,
        remote_control_streams: ReplyStreams::default(),
        gate: Arc::new(crate::injection::InjectionGate::default()),
        store: Arc::new(tokio::sync::RwLock::new(crate::store::RequestStore::new(
            crate::store::retention::RetentionPolicy::default(),
        ))),
    }
}

struct Seeded {
    path: String,
    ids: [i64; 2],
}

async fn seed(persist: &PersistHandle) -> anyhow::Result<Seeded> {
    let path = format!("/tmp/recall-floor-{}", uuid::Uuid::new_v4());
    let project_id = seed_project(persist.pool(), &path).await?;
    let mut ids = [0_i64; 2];
    for (slot, text) in ids.iter_mut().zip([FIRST, SECOND]) {
        let row = persist
            .lessons()
            .create_lesson_with_topic(NewLessonWrite {
                project_id,
                kind: "convention",
                text,
                topic: Some("deploy"),
                source_turn_id: None,
                agent_id: None,
                session_db_id: None,
                request_id: None,
            })
            .await?;
        *slot = row.id;
    }
    Ok(Seeded { path, ids })
}

fn env(persist: &PersistHandle, server: &wiremock::MockServer) -> anyhow::Result<DecisionEnv> {
    let client = OpenRouterClient::new(
        "sk-test".to_owned(),
        "typesafe/jev-1.14".to_owned(),
        Some(server.uri()),
        None,
    )?;
    Ok(DecisionEnv::new(
        client,
        persist.clone(),
        true,
        Arc::new(SettingsOverlay::default()),
    ))
}

async fn recalled(
    persist: &PersistHandle,
    path: &str,
    decisions: Option<&DecisionEnv>,
) -> anyhow::Result<RecallOutcome> {
    let raw = serde_json::to_vec(&json!({
        "messages": [
            {"role": "user", "content": "check the deploy"},
            {"role": "assistant", "content": "<mem:recall topic=\"deploy\"/>"},
            {"role": "user", "content": [{"type": "text", "text": "ok"}]}
        ]
    }))?;
    let cfg = RecallInjectorConfig {
        enabled: true,
        max_lessons_per_topic: 5,
    };
    let persist = Some(persist.clone());
    Ok(apply_recall_injection_with(
        RecallInjectionRequest {
            raw: &raw,
            project_path: path,
            persist: &persist,
            cfg: &cfg,
            request_id: RequestId::next(),
            session_id: &uuid::Uuid::new_v4().to_string(),
            push_deps: &push_deps(),
        },
        decisions,
    )
    .await)
}

fn injected_text(outcome: RecallOutcome) -> anyhow::Result<String> {
    match outcome {
        RecallOutcome::Injected { new_body, .. } => {
            Ok(serde_json::from_slice::<Value>(&new_body)?.to_string())
        }
        RecallOutcome::Declined(reason) => anyhow::bail!("recall declined: {reason}"),
    }
}

#[tokio::test]
async fn recall_with_the_point_off_injects_every_lesson() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let seeded = seed(&persist).await?;
    let text = injected_text(recalled(&persist, &seeded.path, None).await?)?;
    assert!(text.contains(FIRST) && text.contains(SECOND));
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn recall_with_the_point_on_injects_only_lessons_at_the_floor() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let seeded = seed(&persist).await?;
    let [first, second] = seeded.ids;
    let server = stub(200, reply(&[(first, 0.9), (second, 0.1)])).await;
    let env = env(&persist, &server)?;
    let text = injected_text(recalled(&persist, &seeded.path, Some(&env)).await?)?;
    assert!(text.contains(FIRST) && !text.contains(SECOND));
    let bodies = asked_bodies(&server).await;
    assert_eq!(bodies.len(), 1);
    assert!(bodies[0].contains("mem:recall topic"));
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn recall_with_every_lesson_below_the_floor_declines() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let seeded = seed(&persist).await?;
    let [first, second] = seeded.ids;
    let server = stub(200, reply(&[(first, 0.1), (second, 0.2)])).await;
    let env = env(&persist, &server)?;
    let outcome = recalled(&persist, &seeded.path, Some(&env)).await?;
    assert!(matches!(
        outcome,
        RecallOutcome::Declined("recall_lessons_below_floor")
    ));
    persist.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn recall_with_a_failing_call_injects_every_lesson() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let seeded = seed(&persist).await?;
    let server = stub(500, json!({})).await;
    let env = env(&persist, &server)?;
    let text = injected_text(recalled(&persist, &seeded.path, Some(&env)).await?)?;
    assert!(text.contains(FIRST) && text.contains(SECOND));
    persist.pool().close().await;
    db.cleanup().await
}
