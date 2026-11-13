use crate::store::persistence::runtime_repo::RuntimePersist as _;
use serde_json::json;
use uuid::Uuid;

use super::ingest::{self, ObserveSource};
use crate::store::persistence::{PersistHandle, test_support};

fn source() -> ObserveSource {
    ObserveSource {
        label: format!("test:{}", Uuid::new_v4()),
        codex_version: Some("0.167.1".into()),
        hook_boundary: true,
        codex_home: None,
    }
}

async fn database() -> anyhow::Result<Option<(test_support::TestDbHandle, PersistHandle)>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    Ok(Some((db, persist)))
}

#[tokio::test]
async fn hook_registers_common_session_and_replays_without_raw_payload() -> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let source = source();
    let id = Uuid::new_v4();
    let delivery = Uuid::new_v4();
    let body = serde_json::to_vec(&json!({
        "session_id": id, "hook_event_name": "SessionStart", "model": "requested-model",
        "cwd": "/tmp/mike-codex-fixture", "prompt": "sensitive fixture prompt"
    }))?;
    let first = ingest::ingest_hook(&persist, &source, delivery, &body).await?;
    let again = ingest::ingest_hook(&persist, &source, delivery, &body).await?;
    assert_eq!(first.mike_session_id, Some(id));
    assert_eq!(first.control, "hook_boundary");
    assert!(!first.replayed && again.replayed);
    let rows = persist.runtime().session_observations(&[id]).await?;
    let row = rows
        .first()
        .ok_or_else(|| anyhow::anyhow!("no common observation"))?;
    assert_eq!(row.runtime, "codex");
    assert_eq!(row.lifecycle.as_deref(), Some("idle"));
    assert_eq!(row.requested_model.as_deref(), Some("requested-model"));
    assert_eq!(row.model_call_count, 0);
    assert_eq!(row.total_tokens, None);
    assert!(
        persist
            .runtime()
            .runtime_observation(first.namespace_id, delivery)
            .await?
            .is_none()
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn tool_metadata_upserts_without_retaining_input_or_output() -> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let source = source();
    let id = Uuid::new_v4();
    for event in ["PreToolUse", "PostToolUse"] {
        let body = serde_json::to_vec(&json!({
            "session_id":id,"turn_id":"turn","hook_event_name":event,
            "tool_use_id":"tool-1","tool_name":"Bash",
            "tool_input":{"command":"sensitive fixture command"},
            "tool_response":"sensitive fixture output"
        }))?;
        ingest::ingest_hook(&persist, &source, Uuid::new_v4(), &body).await?;
    }
    let items = persist.runtime().list_runtime_items(id, 20, 0).await?;
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.item_id, "tool-1");
    assert_eq!(item.status.as_deref(), Some("completed"));
    assert_eq!(item.content_availability, "not_collected");
    assert!(!serde_json::to_string(&items)?.contains("sensitive fixture"));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn child_hook_uses_distinct_session_and_stops_only_child() -> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let source = source();
    let root = Uuid::new_v4();
    let child = Uuid::new_v4();
    for body in [
        json!({"session_id":root,"hook_event_name":"UserPromptSubmit"}),
        json!({"session_id":root,"agent_id":child,"hook_event_name":"SubagentStop"}),
    ] {
        ingest::ingest_hook(
            &persist,
            &source,
            Uuid::new_v4(),
            &serde_json::to_vec(&body)?,
        )
        .await?;
    }
    let rows = persist
        .runtime()
        .session_observations(&[root, child])
        .await?;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .find(|r| r.session_uuid == root)
            .and_then(|r| r.lifecycle.as_deref()),
        Some("active")
    );
    assert_eq!(
        rows.iter()
            .find(|r| r.session_uuid == child)
            .and_then(|r| r.lifecycle.as_deref()),
        Some("ended")
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn malformed_and_oversized_observations_are_rejected() -> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let source = source();
    let bad = ingest::ingest_hook(&persist, &source, Uuid::new_v4(), b"{\"session_id\": 1").await;
    assert!(matches!(bad, Err(ingest::IngestError::BadInput(ref m)) if m.contains("column")));
    let large = ingest::ingest_hook(
        &persist,
        &source,
        Uuid::new_v4(),
        &vec![b' '; 512 * 1024 + 1],
    )
    .await;
    assert!(matches!(large, Err(ingest::IngestError::BadInput(ref m)) if m.contains("limit")));
    let binary = ingest::ingest_otel(&persist, &source, "logs", b"\x0a\x01binary").await;
    assert!(
        matches!(binary, Err(ingest::IngestError::BadInput(ref m)) if m.contains("protocol = \"json\""))
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn verified_home_maps_hook_to_managed_seat_and_keeps_other_home_distinct()
-> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let first_home = tempfile::tempdir()?;
    let second_home = tempfile::tempdir()?;
    let thread = Uuid::new_v4().to_string();
    let first_seat = Uuid::new_v4();
    let second_seat = Uuid::new_v4();
    for (home, seat) in [
        (first_home.path(), first_seat),
        (second_home.path(), second_seat),
    ] {
        crate::launch::codex::bind_seat_thread(&persist, home, seat, &thread, None).await?;
        let mut source = source();
        source.codex_home = Some(home.to_string_lossy().into_owned());
        source.hook_boundary = false;
        let body =
            serde_json::to_vec(&json!({"session_id":thread,"hook_event_name":"SessionStart"}))?;
        let report = ingest::ingest_hook(&persist, &source, Uuid::new_v4(), &body).await?;
        assert_eq!(report.mike_session_id, Some(seat));
        assert_eq!(
            super::resolve_bound_session(&persist, home, &thread).await?,
            Some(seat)
        );
    }
    let unknown_home = tempfile::tempdir()?;
    assert!(
        super::resolve_bound_session(&persist, unknown_home.path(), &thread)
            .await?
            .is_none()
    );
    let provider_uuid = Uuid::parse_str(&thread)?;
    assert!(
        persist
            .runtime()
            .runtime_thread_by_session(provider_uuid)
            .await?
            .is_none()
    );
    assert_eq!(
        persist
            .runtime()
            .session_observations(&[first_seat, second_seat])
            .await?
            .len(),
        2
    );
    db.cleanup().await?;
    Ok(())
}
