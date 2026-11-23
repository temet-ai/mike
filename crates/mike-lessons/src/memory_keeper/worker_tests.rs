use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

use super::{
    BUSY_BACKSTOP, BUSY_SOURCES, BUSY_SOURCES_CURRENT, Keeper, LEASE_ROLE, TICK, TickOutcome,
};
use crate::change_feed::{ChangeFeed, spawn_change_feed};
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_session_turns,
};
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

struct Fed {
    feed: ChangeFeed,
    shutdown: CancellationToken,
    _events: broadcast::Sender<crate::store::models::ProxyEvent>,
}

impl Drop for Fed {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

fn fed(persist: &PersistHandle) -> Fed {
    let feed = ChangeFeed::new();
    let (events, rx) = broadcast::channel(16);
    let shutdown = CancellationToken::new();
    spawn_change_feed(
        &feed,
        Some(persist),
        rx,
        shutdown.clone(),
        &TaskTracker::new(),
    );
    Fed {
        feed,
        shutdown,
        _events: events,
    }
}

async fn tagged_note(f: &Fixture, session_id: i64, title: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO workspace.session_notes (session_id, kind, title, tags)
         VALUES ($1, 'decision', $2, ARRAY['thread:keeper-cards'])",
    )
    .bind(session_id)
    .bind(title)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn card_worker_sweep_with_a_change_feed_waits_for_a_busy_source_notice() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let fed = fed(&f.handle);
    let thread_id = f.checkpoint("busy sources").await?;
    let session_id = f.session_row().await?;
    let start = Instant::now();
    let mut keeper = Keeper::new(start);
    keeper.tick(&f.handle, start).await;
    let now = Utc::now();
    assert!(f.handle.is_fresh(BUSY_SOURCES_CURRENT, &BUSY_SOURCES, now));
    let after_backstop = now + BUSY_BACKSTOP;
    assert!(
        !f.handle
            .is_fresh(BUSY_SOURCES_CURRENT, &BUSY_SOURCES, after_backstop)
    );

    tagged_note(&f, session_id, "Waits for its notice").await?;
    fed.feed.record(RowChangedTable::Requests, None);
    let quiet = start + Duration::from_secs(61);
    assert_eq!(written(keeper.tick(&f.handle, quiet).await), Some(0));

    fed.feed
        .record(RowChangedTable::ThreadCardBusySources, None);
    let woken = start + Duration::from_secs(122);
    assert_eq!(written(keeper.tick(&f.handle, woken).await), Some(1));
    let (text, _) = card(f.handle.pool(), thread_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no card"))?;
    assert!(text.contains("Waits for its notice"), "{text}");
    f.db.cleanup().await
}

const SENTINEL: &str = r#"{"table":"sentinel"}"#;

async fn busy_notices_before_sentinel(listener: &mut PgListener) -> anyhow::Result<usize> {
    let mut seen = 0;
    loop {
        let notice = listener.recv().await?;
        if notice.payload() == SENTINEL {
            return Ok(seen);
        }
        let payload: serde_json::Value = serde_json::from_str(notice.payload())?;
        if payload["table"] == "thread_card_busy_sources" {
            seen += 1;
        }
    }
}

async fn promise(pool: &PgPool, session_id: i64, key: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.dossier_entries
             (session_id, kind, data, status, source, source_ref, dedup_key, extractor_version,
              effective_at)
         VALUES ($1, 'promise', '{}', 'open', 'capture', 'requests.requests:1', $2, 'test', now())",
    )
    .bind(session_id)
    .bind(key)
    .execute(pool)
    .await?;
    Ok(())
}

async fn live_verdict(pool: &PgPool, session_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.turn_verdicts
                (operator_turn_id, session_id, mode, rubric_version, input_builder_version, model,
                 reaction, delivery, confidence, rationale, unmet_ask, evidence_quality, projected,
                 as_of, input_text)
         SELECT t.id, t.session_id, 'live', 'test', 'test', 'test', 'accepted', 'none', 0.9, 'r',
                '', 'absent', true, NOW(), 'input'
           FROM memory.operator_turns t WHERE t.session_id = $1 LIMIT 1",
    )
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn busy_sources_notify_only_for_thread_linked_sessions_and_tagged_notes() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.checkpoint("notices").await?;
    let linked = f.session_row().await?;
    let pool = f.handle.pool();
    seed_session_turns(pool, &f.session.to_string(), f.project, 1).await?;
    let unlinked: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, now(), now()) RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(f.project)
    .fetch_one(pool)
    .await?;
    let mut listener = PgListener::connect(&f.db.database_url).await?;
    listener.listen("proxy.row_changed").await?;

    promise(pool, unlinked, "unlinked").await?;
    sqlx::query(
        "INSERT INTO workspace.session_notes (session_id, kind, title, tags)
         VALUES ($1, 'decision', 'Untagged', '{}')",
    )
    .bind(linked)
    .execute(pool)
    .await?;
    promise(pool, linked, "linked").await?;
    tagged_note(&f, linked, "Tagged").await?;
    live_verdict(pool, linked).await?;
    sqlx::query("SELECT pg_notify('proxy.row_changed', $1)")
        .bind(SENTINEL)
        .execute(pool)
        .await?;

    assert_eq!(busy_notices_before_sentinel(&mut listener).await?, 3);
    f.db.cleanup().await
}
