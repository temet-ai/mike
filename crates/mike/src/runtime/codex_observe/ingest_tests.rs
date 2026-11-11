use serde_json::Value;
use uuid::Uuid;

use super::ingest::{self, ObserveSource, namespace_for};
use super::parse_tests::{POLL_HOOKS, POLL_LOGS, SUBAGENT_HOOKS, SUBAGENT_LOGS, SUBAGENT_SPANS};
use super::reads;
use crate::store::persistence::{PersistHandle, test_support};

fn source() -> ObserveSource {
    ObserveSource {
        label: format!("test:{}", Uuid::new_v4()),
        codex_version: Some("0.167.1".into()),
    }
}

async fn replay_hooks(
    persist: &PersistHandle,
    source: &ObserveSource,
    lines: &str,
) -> anyhow::Result<()> {
    for line in lines.lines().filter(|line| !line.trim().is_empty()) {
        ingest::ingest_hook(persist, source, Uuid::new_v4(), line.as_bytes()).await?;
    }
    Ok(())
}

async fn replay_otel(
    persist: &PersistHandle,
    source: &ObserveSource,
    signal: &str,
    exports: &str,
) -> anyhow::Result<()> {
    let bodies: Vec<Value> = serde_json::from_str(exports)?;
    for body in bodies {
        ingest::ingest_otel(persist, source, signal, &serde_json::to_vec(&body)?).await?;
    }
    Ok(())
}

async fn database() -> anyhow::Result<Option<(test_support::TestDbHandle, PersistHandle)>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    Ok(Some((db, persist)))
}

#[tokio::test]
async fn codex_observe_subagent_run_registers_parent_and_child_threads() -> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let source = source();
    replay_otel(&persist, &source, "logs", SUBAGENT_LOGS).await?;
    replay_hooks(&persist, &source, SUBAGENT_HOOKS).await?;
    let ns = namespace_for(&source).id;
    let threads: Vec<_> = reads::list_threads(persist.pool(), None, 500, 0)
        .await?
        .into_iter()
        .filter(|t| t.namespace_id == ns)
        .collect();
    let root = threads
        .iter()
        .find(|t| t.kind == "session")
        .ok_or_else(|| anyhow::anyhow!("no root"))?;
    let child = threads
        .iter()
        .find(|t| t.kind == "subagent")
        .ok_or_else(|| anyhow::anyhow!("no child"))?;
    assert_eq!(threads.len(), 2, "{threads:#?}");
    assert_eq!(
        child.parent_thread_id.as_deref(),
        Some(root.thread_id.as_str())
    );
    assert_eq!(child.root_thread_id, root.thread_id);
    assert_eq!(child.agent_type.as_deref(), Some("default"));
    assert_eq!(root.end_reason.as_deref(), Some("other"));
    assert_eq!(root.codex_version.as_deref(), Some("0.167.1"));
    assert_eq!(
        child.sources,
        vec!["hook".to_owned(), "telemetry".to_owned()]
    );
    assert!(!root.managed_by_mike && !child.managed_by_mike);
    assert!(child.tools >= 1 && child.turns == 1, "{child:#?}");
    assert!(root.hook_model_requests >= 4, "{root:#?}");
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn codex_observe_tool_seen_by_hook_and_telemetry_is_one_row() -> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let source = source();
    replay_hooks(&persist, &source, POLL_HOOKS).await?;
    replay_otel(&persist, &source, "logs", POLL_LOGS).await?;
    let ns = namespace_for(&source).id;
    let threads = reads::list_threads(persist.pool(), None, 500, 0).await?;
    let ids: Vec<String> = threads
        .iter()
        .filter(|t| t.namespace_id == ns)
        .map(|t| t.thread_id.clone())
        .collect();
    let tools = reads::list_tools(persist.pool(), ns, &ids).await?;
    let shell: Vec<_> = tools.iter().filter(|t| t.kind == "shell").collect();
    assert_eq!(shell.len(), 2, "{tools:#?}");
    for call in &shell {
        assert_eq!(
            call.sources,
            vec!["hook".to_owned(), "telemetry".to_owned()]
        );
        assert_eq!(call.hook_tool_name.as_deref(), Some("Bash"));
        assert_eq!(call.telemetry_tool_name.as_deref(), Some("exec_command"));
        assert!(
            call.hook_response_seen && call.telemetry_output_snippet && call.completed_at.is_some()
        );
    }
    let long = shell
        .iter()
        .find(|t| t.process_session.is_some())
        .ok_or_else(|| anyhow::anyhow!("no long command"))?;
    let polls: Vec<_> = tools.iter().filter(|t| t.kind == "poll").collect();
    assert_eq!(polls.len(), 3);
    assert!(
        polls
            .iter()
            .all(|p| p.process_session == long.process_session
                && p.sources == vec!["telemetry".to_owned()])
    );
    assert_eq!(tools.iter().filter(|t| t.kind == "code_mode").count(), 5);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn codex_observe_replayed_delivery_changes_nothing() -> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let source = source();
    let line = POLL_HOOKS
        .lines()
        .find(|l| l.contains("\"PostModelRequest\""))
        .ok_or_else(|| anyhow::anyhow!("no line"))?;
    let delivery = Uuid::new_v4();
    let first = ingest::ingest_hook(&persist, &source, delivery, line.as_bytes()).await?;
    let again = ingest::ingest_hook(&persist, &source, delivery, line.as_bytes()).await?;
    assert!(!first.replayed && first.facts == 3 && first.control == "none");
    assert!(again.replayed && again.facts == 0);
    let threads = reads::list_threads(persist.pool(), None, 500, 0).await?;
    let root = threads
        .iter()
        .find(|t| t.namespace_id == first.namespace_id)
        .ok_or_else(|| anyhow::anyhow!("no row"))?;
    assert_eq!(root.hook_model_requests, 1);
    let body: Value = serde_json::from_str(POLL_LOGS)?;
    let export = serde_json::to_vec(&body[0])?;
    let a = ingest::ingest_otel(&persist, &source, "logs", &export).await?;
    let b = ingest::ingest_otel(&persist, &source, "logs", &export).await?;
    assert!(!a.replayed && b.replayed && a.ingestion_id == b.ingestion_id);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn codex_observe_hook_deadlines_and_bad_bodies_are_reported() -> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    let source = source();
    replay_otel(&persist, &source, "traces", SUBAGENT_SPANS).await?;
    let ns = namespace_for(&source).id;
    let timeouts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM runtime.codex_observed_hook_runs WHERE namespace_id = $1 AND outcome = 'timeout'",
    )
    .bind(ns)
    .fetch_one(persist.pool())
    .await?;
    assert_eq!(timeouts, 5);
    let bad = ingest::ingest_hook(&persist, &source, Uuid::new_v4(), b"{\"session_id\": 1").await;
    assert!(
        matches!(bad, Err(ingest::IngestError::BadInput(ref m)) if m.contains("column")),
        "{bad:?}"
    );
    let not_json = ingest::ingest_otel(&persist, &source, "logs", b"\x0a\x01binary").await;
    assert!(
        matches!(not_json, Err(ingest::IngestError::BadInput(ref m)) if m.contains("protocol = \"json\""))
    );
    db.cleanup().await?;
    Ok(())
}
