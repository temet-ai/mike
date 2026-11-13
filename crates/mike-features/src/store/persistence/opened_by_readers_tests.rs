use super::*;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::test_support;

const PROJECT: &str = "w-opened-by-readers";
const SESSION: &str = "00000000-0000-4000-8000-0000000000e1";
const OTHER_SESSION: &str = "00000000-0000-4000-8000-0000000000e2";

struct Fixture {
    db: test_support::TestDbHandle,
    handle: PersistHandle,
    project_id: i64,
    machine: i64,
    operator: i64,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = handle.pool();
    let project_id = test_support::seed_project(pool, PROJECT).await?;
    test_support::seed_two_sessions(pool, SESSION, OTHER_SESSION, project_id).await?;
    test_support::seed_session_turns(pool, SESSION, project_id, 2).await?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM memory.operator_turns WHERE project_id = $1 ORDER BY id",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    let &[machine, operator] = &ids[..] else {
        anyhow::bail!("expected two seeded turns, got {ids:?}");
    };
    sqlx::query(
        "UPDATE memory.operator_turns
            SET opened_by = CASE WHEN id = $1 THEN 'machine' ELSE 'operator' END,
                user_prompt = 'zebra needle ' || id,
                closed_at = NOW() - interval '10 hours'
          WHERE id = ANY($2)",
    )
    .bind(machine)
    .bind(&ids)
    .execute(pool)
    .await?;
    Ok(Some(Fixture {
        db,
        handle,
        project_id,
        machine,
        operator,
    }))
}

async fn add_chunk(pool: &sqlx::PgPool, turn_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.operator_turn_chunks
            (turn_id, chunk_idx, chunk_role_id, chunk_text, chunk_embedding, embedding_model)
         SELECT $1, 0, cr.id, 'zebra needle chunk',
                ('[' || array_to_string(array_fill(0.1::float4, ARRAY[768]), ',') || ']')::vector,
                'test-model'
           FROM memory.chunk_roles cr
          WHERE cr.code = 'prompt'",
    )
    .bind(turn_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn unjudged_backlog_never_counts_machine_rows() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(6);
    let backlog = fx.handle.unjudged_closed_turn_backlog(cutoff).await?;
    assert_eq!(
        backlog.stale_turns, 1,
        "only the operator row {} counts, not {}",
        fx.operator, fx.machine
    );
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn embedding_reads_skip_machine_rows() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    assert!(
        fx.handle
            .get_turn_for_embedding(fx.machine)
            .await?
            .is_none()
    );
    assert!(
        fx.handle
            .get_turn_for_embedding(fx.operator)
            .await?
            .is_some()
    );
    let pending = fx.handle.fetch_unembedded_turn_ids(10).await?;
    assert_eq!(pending, vec![fx.operator]);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn content_search_skips_chunks_of_machine_rows() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    add_chunk(fx.handle.pool(), fx.machine).await?;
    add_chunk(fx.handle.pool(), fx.operator).await?;
    let totals =
        content_search::search_content_totals(fx.handle.pool(), "zebra needle", None).await?;
    assert_eq!(totals.total_matches, 1);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_reads_skip_machine_rows() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    add_chunk(fx.handle.pool(), fx.machine).await?;
    add_chunk(fx.handle.pool(), fx.operator).await?;
    let project = fx
        .handle
        .knowledge()
        .resolve_knowledge_project(PROJECT)
        .await?;
    let fts = fx
        .handle
        .knowledge()
        .recall_turns_by_fts(&project, "zebra", 10, None)
        .await?;
    let ids: Vec<i64> = fts.iter().map(|row| row.turn_id).collect();
    assert_eq!(ids, vec![fx.operator]);
    let vector = format!("[{}]", vec!["0.1"; 768].join(","));
    let semantic = fx
        .handle
        .knowledge()
        .recall_turns_by_embedding(&project, "test-model", &vector, 10, None)
        .await?;
    let semantic_ids: Vec<i64> = semantic.iter().flatten().map(|row| row.turn_id).collect();
    assert_eq!(semantic_ids, vec![fx.operator]);
    let _ = fx.project_id;
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn project_knowledge_search_skips_machine_turns() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let query = project_knowledge::KnowledgeQuery {
        project: PROJECT.to_string(),
        query: "zebra".to_string(),
        kind: Some("turn".to_string()),
        include_historical: true,
        limit: Some(10),
        offset: None,
        ..Default::default()
    };
    let found = fx
        .handle
        .knowledge()
        .search_project_knowledge(&query)
        .await?;
    let references: Vec<String> = found["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["reference"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(references, vec![format!("turn:{}", fx.operator)]);
    fx.db.cleanup().await?;
    Ok(())
}

async fn add_file_touch(pool: &sqlx::PgPool, turn_id: i64, file: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO tools.uses
                (wire_tool_use_id, request_id, session_id, ord, tool_name, input, file_path, operation)
         SELECT 'toolu_file_' || t.id, t.start_request_id, r.session_id, 0, 'Edit',
                jsonb_build_object('file_path', $2::text), $2, 'edit'
           FROM memory.operator_turns t
           JOIN requests.requests r ON r.id = t.start_request_id
          WHERE t.id = $1",
    )
    .bind(turn_id)
    .bind(file)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn recall_by_file_skips_machine_rows() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let file = "src/zebra_needle.rs";
    add_file_touch(fx.handle.pool(), fx.machine, file).await?;
    add_file_touch(fx.handle.pool(), fx.operator, file).await?;
    let project = fx
        .handle
        .knowledge()
        .resolve_knowledge_project(PROJECT)
        .await?;
    let rows = fx
        .handle
        .knowledge()
        .recall_turns_by_file(&project, file, 10)
        .await?;
    let ids: Vec<i64> = rows.iter().map(|row| row.turn_id).collect();
    assert_eq!(
        ids,
        vec![fx.operator],
        "machine row {} must not be recalled by file",
        fx.machine
    );
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn pending_embedding_count_skips_machine_rows() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let pending = fx.handle.count_pending_embedding_turns().await?;
    assert_eq!(
        pending, 1,
        "only operator row {} is pending, not machine row {}",
        fx.operator, fx.machine
    );
    fx.db.cleanup().await?;
    Ok(())
}
