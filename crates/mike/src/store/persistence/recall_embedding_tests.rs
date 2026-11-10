use super::*;
use crate::store::persistence::test_support;

const PROJECT: &str = "w-recall-embedding";
const SESSION: &str = "00000000-0000-4000-8000-0000000000f1";
const OTHER_SESSION: &str = "00000000-0000-4000-8000-0000000000f2";
const NOMIC_MODEL: &str = "nomic-embed-text-v1.5";
const GEMMA_MODEL: &str = "embeddinggemma:300m";
const UNKNOWN_MODEL: &str = "recall-model";

fn angled_literal(similarity: f64) -> String {
    let mut vector = vec![0.0_f64; 768];
    vector[0] = similarity;
    vector[1] = (1.0 - similarity * similarity).sqrt();
    let parts: Vec<String> = vector.iter().map(|v| format!("{v:.6}")).collect();
    format!("[{}]", parts.join(","))
}

struct Fixture {
    db: test_support::TestDbHandle,
    handle: PersistHandle,
    turns: Vec<i64>,
}

async fn fixture(chunks: &[(usize, f64, &str)]) -> anyhow::Result<Option<Fixture>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = handle.pool();
    let project_id = test_support::seed_project(pool, PROJECT).await?;
    test_support::seed_two_sessions(pool, SESSION, OTHER_SESSION, project_id).await?;
    test_support::seed_session_turns(pool, SESSION, project_id, 3).await?;
    let turns: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM memory.operator_turns WHERE project_id = $1 ORDER BY id",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    for (turn_index, similarity, model) in chunks {
        add_chunk(pool, turns[*turn_index], *similarity, model).await?;
    }
    Ok(Some(Fixture { db, handle, turns }))
}

async fn add_chunk(
    pool: &sqlx::PgPool,
    turn_id: i64,
    similarity: f64,
    model: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.operator_turn_chunks
            (turn_id, chunk_idx, chunk_role_id, chunk_text, chunk_embedding, embedding_model)
         SELECT $1, 0, cr.id, 'recall chunk', $2::text::vector, $3
           FROM memory.chunk_roles cr
          WHERE cr.code = 'prompt'",
    )
    .bind(turn_id)
    .bind(angled_literal(similarity))
    .bind(model)
    .execute(pool)
    .await?;
    Ok(())
}

async fn recalled_with(fx: &Fixture, model: &str) -> anyhow::Result<Vec<i64>> {
    let project = fx.handle.resolve_knowledge_project(PROJECT).await?;
    let rows = fx
        .handle
        .recall_turns_by_embedding(&project, model, &angled_literal(1.0), 10)
        .await?;
    Ok(rows.into_iter().flatten().map(|row| row.turn_id).collect())
}

async fn recalled(fx: &Fixture) -> anyhow::Result<Vec<i64>> {
    recalled_with(fx, NOMIC_MODEL).await
}

#[tokio::test]
async fn recall_turns_by_embedding_nomic_model_keeps_similarity_062_and_cuts_059()
-> anyhow::Result<()> {
    let Some(fx) = fixture(&[(0, 0.62, NOMIC_MODEL), (1, 0.59, NOMIC_MODEL)]).await? else {
        return Ok(());
    };
    assert_eq!(recalled_with(&fx, NOMIC_MODEL).await?, vec![fx.turns[0]]);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_turns_by_embedding_embeddinggemma_model_keeps_similarity_05_and_cuts_04()
-> anyhow::Result<()> {
    let Some(fx) = fixture(&[(0, 0.50, GEMMA_MODEL), (1, 0.40, GEMMA_MODEL)]).await? else {
        return Ok(());
    };
    assert_eq!(recalled_with(&fx, GEMMA_MODEL).await?, vec![fx.turns[0]]);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_turns_by_embedding_unknown_model_keeps_every_positive_similarity()
-> anyhow::Result<()> {
    let Some(fx) = fixture(&[(0, 0.30, UNKNOWN_MODEL), (1, 0.05, UNKNOWN_MODEL)]).await? else {
        return Ok(());
    };
    assert_eq!(
        recalled_with(&fx, UNKNOWN_MODEL).await?,
        vec![fx.turns[0], fx.turns[1]]
    );
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_turns_by_embedding_chunk_of_another_model_is_excluded() -> anyhow::Result<()> {
    let Some(fx) = fixture(&[(0, 0.90, NOMIC_MODEL), (1, 0.99, "other-model")]).await? else {
        return Ok(());
    };
    assert_eq!(recalled(&fx).await?, vec![fx.turns[0]]);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_turns_by_embedding_hit_more_than_the_window_below_the_best_is_dropped()
-> anyhow::Result<()> {
    let Some(fx) = fixture(&[
        (0, 0.84, NOMIC_MODEL),
        (1, 0.78, NOMIC_MODEL),
        (2, 0.70, NOMIC_MODEL),
    ])
    .await?
    else {
        return Ok(());
    };
    assert_eq!(recalled(&fx).await?, vec![fx.turns[0], fx.turns[1]]);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_turns_by_embedding_best_hit_below_the_absolute_floor_returns_nothing()
-> anyhow::Result<()> {
    let Some(fx) = fixture(&[(0, 0.55, NOMIC_MODEL), (1, 0.52, NOMIC_MODEL)]).await? else {
        return Ok(());
    };
    assert!(recalled(&fx).await?.is_empty());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_turns_by_embedding_turn_with_several_chunks_is_returned_once() -> anyhow::Result<()>
{
    let Some(fx) = fixture(&[(0, 0.90, NOMIC_MODEL)]).await? else {
        return Ok(());
    };
    sqlx::query(
        "INSERT INTO memory.operator_turn_chunks
            (turn_id, chunk_idx, chunk_role_id, chunk_text, chunk_embedding, embedding_model)
         SELECT $1, 1, cr.id, 'second chunk', $2::text::vector, $3
           FROM memory.chunk_roles cr
          WHERE cr.code = 'prompt'",
    )
    .bind(fx.turns[0])
    .bind(angled_literal(0.88))
    .bind(NOMIC_MODEL)
    .execute(fx.handle.pool())
    .await?;
    assert_eq!(recalled(&fx).await?, vec![fx.turns[0]]);
    fx.db.cleanup().await?;
    Ok(())
}
