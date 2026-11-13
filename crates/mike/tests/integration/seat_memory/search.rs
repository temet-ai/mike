use mike::store::persistence::PersistHandle;
use mike::store::persistence::knowledge_repo::KnowledgePersist as _;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use mike::store::persistence::project_knowledge::KnowledgeQuery;
use mike::store::test_support::{TestDb, TestDbHandle};
use serde_json::Value;

use super::fixture::session;
use super::scenario::{Scenario, build};

struct Indexed {
    db: TestDbHandle,
    handle: PersistHandle,
    project: String,
    scenario: Scenario,
}

async fn indexed() -> anyhow::Result<Option<Indexed>> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let scenario = build(handle.pool()).await?;
    handle.launch().refresh_seat_memory_items().await?;
    let project: String =
        sqlx::query_scalar("SELECT project_uuid::text FROM projects.projects WHERE id = $1")
            .bind(scenario.project_id)
            .fetch_one(handle.pool())
            .await?;
    Ok(Some(Indexed {
        db,
        handle,
        project,
        scenario,
    }))
}

async fn search(fx: &Indexed, query: &str, kind: Option<&str>) -> anyhow::Result<Vec<Value>> {
    let found = fx
        .handle
        .knowledge()
        .search_project_knowledge(&KnowledgeQuery {
            project: fx.project.clone(),
            query: query.into(),
            kind: kind.map(str::to_owned),
            limit: Some(100),
            ..KnowledgeQuery::default()
        })
        .await?;
    Ok(found["items"].as_array().cloned().unwrap_or_default())
}

async fn finish(fx: Indexed) -> anyhow::Result<()> {
    fx.handle.pool().close().await;
    fx.db.cleanup().await
}

#[tokio::test]
async fn seat_report_search_hit_carries_seat_labels_and_reads_in_full() -> anyhow::Result<()> {
    let Some(fx) = indexed().await? else {
        return Ok(());
    };
    let items = search(&fx, "SM-REPORT-TURN", None).await?;
    assert_eq!(items.len(), 1, "{items:?}");
    let hit = &items[0];
    assert_eq!(hit["source_kind"], "seat");
    assert_eq!(hit["status"], "seat_report");
    assert_eq!(hit["author"]["role"], "seat");
    assert_eq!(
        hit["author"]["session_uuid"],
        fx.scenario.worker.session_uuid
    );
    assert_eq!(hit["provenance"]["tier"], "seat");
    assert_eq!(hit["provenance"]["run_id"], fx.scenario.run);
    assert!(
        hit["provenance"]["framing"]
            .as_str()
            .is_some_and(|f| f.contains("not by the operator"))
    );
    let reference = hit["reference"].as_str().unwrap_or_default();
    let read = fx
        .handle
        .knowledge()
        .read_project_knowledge(None, reference)
        .await?;
    assert!(
        read["body"]
            .as_str()
            .is_some_and(|b| b.contains("SM-REPORT-TURN")),
        "{read}"
    );
    finish(fx).await
}

#[tokio::test]
async fn seat_brief_search_hit_reads_its_launch() -> anyhow::Result<()> {
    let Some(fx) = indexed().await? else {
        return Ok(());
    };
    let items = search(&fx, "SM-BRIEF-WORKER", Some("seat")).await?;
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["status"], "seat_brief");
    assert_eq!(items[0]["author"]["role"], "coordinator");
    let reference = items[0]["reference"].as_str().unwrap_or_default();
    assert!(reference.starts_with("launch:"), "{reference}");
    let read = fx
        .handle
        .knowledge()
        .read_project_knowledge(Some(&fx.project), reference)
        .await?;
    assert_eq!(read["body"], "SM-BRIEF-WORKER");
    finish(fx).await
}

#[tokio::test]
async fn seat_search_never_returns_excluded_material() -> anyhow::Result<()> {
    let Some(fx) = indexed().await? else {
        return Ok(());
    };
    let items = search(&fx, "", Some("seat")).await?;
    assert!(!items.is_empty());
    for item in &items {
        let excerpt = item["excerpt"].as_str().unwrap_or_default();
        assert!(
            !excerpt.contains("SM-EXCL"),
            "excluded material reached search: {item}"
        );
    }
    finish(fx).await
}

#[tokio::test]
async fn seat_hit_ranks_below_operator_note_with_the_same_words() -> anyhow::Result<()> {
    let Some(fx) = indexed().await? else {
        return Ok(());
    };
    let operator = session(fx.handle.pool(), fx.scenario.project_id).await?;
    sqlx::query(
        "INSERT INTO workspace.session_notes (session_id, title, body)
         VALUES ($1, 'operator note', 'SM-BRIEF-PEER decided by the operator')",
    )
    .bind(operator.session_id)
    .execute(fx.handle.pool())
    .await?;
    let items = search(&fx, "SM-BRIEF-PEER", None).await?;
    let kinds: Vec<&str> = items
        .iter()
        .filter_map(|i| i["source_kind"].as_str())
        .collect();
    assert_eq!(kinds, ["note", "seat"], "{items:?}");
    finish(fx).await
}

#[tokio::test]
async fn seat_memory_refresh_with_nothing_changed_writes_nothing() -> anyhow::Result<()> {
    let Some(fx) = indexed().await? else {
        return Ok(());
    };
    let again = fx.handle.launch().refresh_seat_memory_items().await?;
    assert!(again.items > 0);
    assert_eq!((again.written, again.removed), (0, 0));
    finish(fx).await
}

#[tokio::test]
async fn seat_memory_generation_moves_when_a_fleet_event_is_added() -> anyhow::Result<()> {
    let Some(fx) = indexed().await? else {
        return Ok(());
    };
    let before = fx.handle.launch().seat_memory_generation().await?;
    super::fixture::event(
        fx.handle.pool(),
        &fx.scenario.run,
        None,
        Some("T9"),
        "report_filed",
        &serde_json::json!({}),
    )
    .await?;
    assert!(fx.handle.launch().seat_memory_generation().await? > before);
    finish(fx).await
}

fn rust_and_sql_files(
    dir: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "migrations") {
                rust_and_sql_files(&path, out)?;
            }
        } else if path.extension().is_some_and(|e| e == "rs" || e == "sql") {
            out.push(path);
        }
    }
    Ok(())
}

#[test]
fn seat_tier_is_read_only_by_knowledge_search() -> anyhow::Result<()> {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_and_sql_files(&src, &mut files)?;
    let mut readers = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file)?;
        if text.contains("memory.seat_memory(") || text.contains("memory.seat_memory_items") {
            let relative = file
                .strip_prefix(&src)?
                .to_string_lossy()
                .replace('\\', "/");
            readers.push(relative);
        }
    }
    readers.sort();
    assert_eq!(
        readers,
        [
            "store/persistence/project_knowledge/search.sql",
            "store/persistence/seat_memory/refresh.sql",
        ],
        "seat material may reach only knowledge search, never judge, proposer or lesson inputs"
    );
    Ok(())
}
