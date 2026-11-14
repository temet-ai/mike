use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::{Keeper, LEASE_ROLE, TICK, TickOutcome};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use crate::store::persistence::work_threads::{CheckpointInput, MetaUpdate, StateFields};

const SLUG: &str = "keeper-cards";

struct Fixture {
    db: TestDbHandle,
    handle: PersistHandle,
    project: i64,
    session: Uuid,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url)
        .await?
        .with_llm_machine(Uuid::new_v4());
    let project = handle
        .knowledge()
        .define_knowledge_project("keeper worker tests", Uuid::new_v4())
        .await?
        .id;
    Ok(Some(Fixture {
        db,
        handle,
        project,
        session: Uuid::new_v4(),
    }))
}

impl Fixture {
    async fn checkpoint(&self, where_we_are: &str) -> anyhow::Result<i64> {
        let outcome = self
            .handle
            .launch()
            .checkpoint_work_thread(
                self.project,
                &CheckpointInput {
                    slug: SLUG,
                    tldr: "keeper cards",
                    state: StateFields {
                        where_we_are,
                        ..StateFields::default()
                    },
                    session_uuid: Some(self.session),
                    commit_sha: None,
                    contribution: None,
                    meta: MetaUpdate {
                        title: Some("Keeper cards"),
                        ..MetaUpdate::default()
                    },
                },
            )
            .await?;
        Ok(outcome.thread.id)
    }

    async fn session_row(&self) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar::<_, i64>(
            "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1, $2, now(), now())
             ON CONFLICT (session_uuid) DO UPDATE SET last_seen = EXCLUDED.last_seen
             RETURNING id",
        )
        .bind(self.session)
        .bind(self.project)
        .fetch_one(self.handle.pool())
        .await?)
    }
}

async fn card(pool: &PgPool, thread_id: i64) -> anyhow::Result<Option<(String, DateTime<Utc>)>> {
    Ok(sqlx::query_as::<_, (String, DateTime<Utc>)>(
        "SELECT text, source_max_at FROM work.thread_cards WHERE thread_id = $1",
    )
    .bind(thread_id)
    .fetch_optional(pool)
    .await?)
}

async fn cards_generation(pool: &PgPool) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT generation FROM memory.change_generations WHERE name = 'thread_cards'",
    )
    .fetch_one(pool)
    .await?)
}

fn written(outcome: TickOutcome) -> Option<usize> {
    match outcome {
        TickOutcome::Built(report) => Some(report.written),
        _ => None,
    }
}

#[tokio::test]
async fn card_worker_after_checkpoint_refreshes_card() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let thread_id = f.checkpoint("the first state of play").await?;
    let mut keeper = Keeper::new(Instant::now());
    assert_eq!(
        written(keeper.tick(&f.handle, Instant::now()).await),
        Some(1)
    );
    let (first, _) = card(f.handle.pool(), thread_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no card"))?;
    assert!(first.contains("the first state of play"), "{first}");

    f.checkpoint("the second state of play").await?;
    assert_eq!(
        written(keeper.tick(&f.handle, Instant::now()).await),
        Some(1)
    );
    let (second, _) = card(f.handle.pool(), thread_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no card"))?;
    assert!(second.contains("the second state of play"), "{second}");
    f.db.cleanup().await
}

#[tokio::test]
async fn card_worker_rebuild_with_unchanged_text_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let thread_id = f.checkpoint("steady state").await?;
    let mut keeper = Keeper::new(Instant::now());
    keeper.tick(&f.handle, Instant::now()).await;
    let pool = f.handle.pool();
    let (_, built_against) = card(pool, thread_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no card"))?;
    let generation = cards_generation(pool).await?;
    sqlx::query("UPDATE work.threads SET updated_at = now() WHERE id = $1")
        .bind(thread_id)
        .execute(pool)
        .await?;

    let outcome = keeper.tick(&f.handle, Instant::now()).await;
    assert!(
        matches!(outcome, TickOutcome::Built(report) if report.rebuilt == 1 && report.written == 0),
        "{outcome:?}"
    );
    assert_eq!(cards_generation(pool).await?, generation);
    let (_, advanced) = card(pool, thread_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no card"))?;
    assert!(advanced > built_against);
    assert_eq!(
        written(keeper.tick(&f.handle, Instant::now()).await),
        Some(0)
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn card_worker_without_lease_builds_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let thread_id = f.checkpoint("held elsewhere").await?;
    let other = PersistHandle::connect(&f.db.database_url)
        .await?
        .with_llm_machine(Uuid::new_v4());
    assert!(other.holds_worker_role(LEASE_ROLE, TICK).await);

    let mut keeper = Keeper::new(Instant::now());
    assert_eq!(
        keeper.tick(&f.handle, Instant::now()).await,
        TickOutcome::NotLeader
    );
    assert!(card(f.handle.pool(), thread_id).await?.is_none());
    f.db.cleanup().await
}

#[tokio::test]
async fn card_worker_sweep_with_tagged_note_adds_only_the_tagged_note() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let thread_id = f.checkpoint("notes arrive").await?;
    let session_id = f.session_row().await?;
    let start = Instant::now();
    let mut keeper = Keeper::new(start);
    keeper.tick(&f.handle, start).await;
    sqlx::query(
        "INSERT INTO workspace.session_notes (session_id, kind, title, tags)
         VALUES ($1, 'decision', 'Tagged decision reaches the card', ARRAY['thread:keeper-cards']),
                ($1, 'decision', 'Untagged decision stays off the card', '{}')",
    )
    .bind(session_id)
    .execute(f.handle.pool())
    .await?;

    let later = start + Duration::from_secs(61);
    assert_eq!(written(keeper.tick(&f.handle, later).await), Some(1));
    let (text, _) = card(f.handle.pool(), thread_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no card"))?;
    assert!(text.contains("Tagged decision reaches the card"), "{text}");
    assert!(!text.contains("Untagged decision"), "{text}");
    f.db.cleanup().await
}

#[tokio::test]
async fn card_worker_after_candidates_are_written_adds_the_related_section() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let thread_id = f.checkpoint("candidates arrive").await?;
    let mut keeper = Keeper::new(Instant::now());
    keeper.tick(&f.handle, Instant::now()).await;
    let uuid: Uuid = sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, topic)
         SELECT $1, lk.id, 'A reranked lesson reaches the card', ls.id, lsrc.id, 'related'
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
          WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'operator'
      RETURNING uuid",
    )
    .bind(f.project)
    .fetch_one(f.handle.pool())
    .await?;
    f.handle
        .launch()
        .replace_thread_card_candidates(
            thread_id,
            &[
                crate::store::persistence::work_threads::NewThreadCandidate {
                    reference: format!("lesson:{uuid}"),
                    kind: "lesson".to_owned(),
                    score: 0.8,
                },
            ],
        )
        .await?;

    assert_eq!(
        written(keeper.tick(&f.handle, Instant::now()).await),
        Some(1)
    );

    let (text, _) = card(f.handle.pool(), thread_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no card"))?;
    assert!(
        text.contains(&format!(
            "Related: A reranked lesson reaches the card [lesson:{uuid}]"
        )),
        "{text}"
    );
    f.db.cleanup().await
}
