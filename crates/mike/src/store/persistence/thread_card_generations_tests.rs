use sqlx::PgPool;
use uuid::Uuid;

use super::PersistHandle;
use super::test_support::{TestDbHandle, acquire_ephemeral_db};
use super::work_threads::{CheckpointInput, MetaUpdate, StateFields};

struct Fixture {
    db: TestDbHandle,
    handle: PersistHandle,
    thread_id: i64,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = handle
        .define_knowledge_project("card generation tests", Uuid::new_v4())
        .await?
        .id;
    let outcome = handle
        .checkpoint_work_thread(
            project,
            &CheckpointInput {
                slug: "card-generations",
                tldr: "first state",
                state: StateFields::default(),
                session_uuid: Some(Uuid::new_v4()),
                commit_sha: None,
                contribution: Some("started it"),
                meta: MetaUpdate {
                    title: Some("Card generations"),
                    ..MetaUpdate::default()
                },
            },
        )
        .await?;
    Ok(Some(Fixture {
        db,
        handle,
        thread_id: outcome.thread.id,
    }))
}

async fn generation(pool: &PgPool, name: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT generation FROM memory.change_generations WHERE name = $1",
    )
    .bind(name)
    .fetch_one(pool)
    .await?)
}

async fn write_card(
    pool: &PgPool,
    thread_id: i64,
    text: &str,
    offset_secs: f64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO work.thread_cards (thread_id, card, text, content_hash, refs, source_max_at)
         VALUES ($1, '{}'::jsonb, $2, md5($2), '[]'::jsonb, now() + make_interval(secs => $3))
         ON CONFLICT (thread_id) DO UPDATE
            SET card = EXCLUDED.card, text = EXCLUDED.text, content_hash = EXCLUDED.content_hash,
                refs = EXCLUDED.refs, source_max_at = EXCLUDED.source_max_at",
    )
    .bind(thread_id)
    .bind(text)
    .bind(offset_secs)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn thread_log_insert_with_new_entry_bumps_card_sources() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let before = generation(pool, "thread_card_sources").await?;
    sqlx::query(
        "INSERT INTO work.thread_log (thread_id, ord, kind, summary, body)
         SELECT $1, max(ord) + 1, 'decision', 'keep cards small', 'keep cards small'
           FROM work.thread_log WHERE thread_id = $1",
    )
    .bind(f.thread_id)
    .execute(pool)
    .await?;
    let after = generation(pool, "thread_card_sources").await?;
    assert_eq!(after, before + 1);
    f.db.cleanup().await
}

#[tokio::test]
async fn thread_session_last_seen_update_alone_leaves_card_sources() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let before = generation(pool, "thread_card_sources").await?;
    sqlx::query(
        "UPDATE work.thread_sessions SET last_seen_at = last_seen_at + interval '1 minute'
          WHERE thread_id = $1",
    )
    .bind(f.thread_id)
    .execute(pool)
    .await?;
    assert_eq!(generation(pool, "thread_card_sources").await?, before);
    f.db.cleanup().await
}

#[tokio::test]
async fn thread_card_upsert_with_unchanged_text_leaves_cards_generation() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    write_card(pool, f.thread_id, "card text", 0.0).await?;
    let before = generation(pool, "thread_cards").await?;
    write_card(pool, f.thread_id, "card text", 60.0).await?;
    assert_eq!(generation(pool, "thread_cards").await?, before);
    write_card(pool, f.thread_id, "changed card text", 120.0).await?;
    assert_eq!(generation(pool, "thread_cards").await?, before + 1);
    f.db.cleanup().await
}

#[tokio::test]
async fn thread_card_write_never_bumps_card_sources() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let before = generation(pool, "thread_card_sources").await?;
    write_card(pool, f.thread_id, "card text", 0.0).await?;
    assert_eq!(generation(pool, "thread_card_sources").await?, before);
    f.db.cleanup().await
}
