mod fixtures;
mod reap;

use crate::test_db;

use chrono::Duration as ChronoDuration;
use chrono::Utc;
use fixtures::{
    EMBED_WORKER_SELECT, TurnSpec, bootstrap_project_session, insert_anchor_request, insert_turn,
};
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn embed_worker_only_fires_on_closed_turns() -> anyhow::Result<()> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping embed_worker_only_fires");
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let (project_id, session_pk) =
        bootstrap_project_session(&pool, "c9-only-closed", &session_id).await?;

    let now = Utc::now();
    let (r_open, _r_open_at) =
        insert_anchor_request(&pool, session_pk, project_id, now, "open turn prompt").await?;
    let (r_closed_pending, _r_closed_pending_at) = insert_anchor_request(
        &pool,
        session_pk,
        project_id,
        now + ChronoDuration::milliseconds(10),
        "closed but un-embedded prompt",
    )
    .await?;
    let (r_done, _r_done_at) = insert_anchor_request(
        &pool,
        session_pk,
        project_id,
        now + ChronoDuration::milliseconds(20),
        "closed and embedded prompt",
    )
    .await?;

    let t_open = insert_turn(
        &pool,
        TurnSpec {
            session_id: session_pk,
            project_id,
            start_request_id: r_open,
            closed_at: None,
            embedded_at: None,
        },
    )
    .await?;

    sqlx::query(
        "UPDATE memory.operator_turns
            SET closed_at = $2, classified_at = $2, embedded_at = $3
          WHERE id = $1",
    )
    .bind(t_open)
    .bind(now + ChronoDuration::milliseconds(5))
    .bind(now + ChronoDuration::milliseconds(6))
    .execute(&pool)
    .await?;

    let t_closed_pending = insert_turn(
        &pool,
        TurnSpec {
            session_id: session_pk,
            project_id,
            start_request_id: r_closed_pending,
            closed_at: Some(now + ChronoDuration::milliseconds(15)),
            embedded_at: None,
        },
    )
    .await?;

    let t_done = insert_turn(
        &pool,
        TurnSpec {
            session_id: session_pk,
            project_id,
            start_request_id: r_done,
            closed_at: Some(now + ChronoDuration::milliseconds(25)),
            embedded_at: Some(now + ChronoDuration::milliseconds(30)),
        },
    )
    .await?;

    let mut tx = pool.begin().await?;
    let rows: Vec<(i64, String, String)> = sqlx::query_as(EMBED_WORKER_SELECT)
        .bind(16i64)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;

    let returned_ids: Vec<i64> = rows.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(
        returned_ids,
        vec![t_closed_pending],
        "embed worker SELECT must surface ONLY the closed + un-embedded turn; got {returned_ids:?}. \
         t_open was filtered by embedded_at NOT NULL (set in cleanup); t_done by embedded_at NOT NULL."
    );

    for excluded in [t_open, t_done] {
        assert!(
            !returned_ids.contains(&excluded),
            "turn {excluded} must NOT appear (its embedded_at is set)"
        );
    }

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn embed_worker_idempotent_via_embedded_at_gate() -> anyhow::Result<()> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping embed_worker_idempotent");
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let (project_id, session_pk) =
        bootstrap_project_session(&pool, "c9-idempotent", &session_id).await?;

    let now = Utc::now();
    let (r_id, _r_created) =
        insert_anchor_request(&pool, session_pk, project_id, now, "idempotency prompt").await?;
    let turn_id = insert_turn(
        &pool,
        TurnSpec {
            session_id: session_pk,
            project_id,
            start_request_id: r_id,
            closed_at: Some(now + ChronoDuration::milliseconds(5)),
            embedded_at: None,
        },
    )
    .await?;

    let first: Vec<(i64, String, String)> = sqlx::query_as(EMBED_WORKER_SELECT)
        .bind(16i64)
        .fetch_all(&pool)
        .await?;
    let first_ids: Vec<i64> = first.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(
        first_ids,
        vec![turn_id],
        "first worker pass must see the closed + un-embedded turn; got {first_ids:?}"
    );

    sqlx::query(
        "UPDATE memory.operator_turns
            SET embedded_at = NOW()
          WHERE id = ANY($1)",
    )
    .bind(&first_ids)
    .execute(&pool)
    .await?;

    let second: Vec<(i64, String, String)> = sqlx::query_as(EMBED_WORKER_SELECT)
        .bind(16i64)
        .fetch_all(&pool)
        .await?;
    assert!(
        second.is_empty(),
        "second worker pass must see no rows after embedded_at is stamped; got {} rows: {:?}",
        second.len(),
        second.iter().map(|(id, _, _)| *id).collect::<Vec<_>>()
    );

    let third: Vec<(i64, String, String)> = sqlx::query_as(EMBED_WORKER_SELECT)
        .bind(16i64)
        .fetch_all(&pool)
        .await?;
    assert!(
        third.is_empty(),
        "third worker pass must remain empty — embedded_at gate is permanent"
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn embed_worker_skips_locked_rows() -> anyhow::Result<()> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping embed_worker_skips_locked");
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&test_db.database_url)
        .await?;

    let session_id_a = uuid::Uuid::new_v4().to_string();
    let session_id_b = uuid::Uuid::new_v4().to_string();
    let (project_id_a, session_pk_a) =
        bootstrap_project_session(&pool, "c9-skiplocked-a", &session_id_a).await?;
    let (project_id_b, session_pk_b) =
        bootstrap_project_session(&pool, "c9-skiplocked-b", &session_id_b).await?;

    let now = Utc::now();
    let (r_a_id, _r_a_at) =
        insert_anchor_request(&pool, session_pk_a, project_id_a, now, "alpha").await?;
    let (r_b_id, _r_b_at) = insert_anchor_request(
        &pool,
        session_pk_b,
        project_id_b,
        now + ChronoDuration::milliseconds(10),
        "beta",
    )
    .await?;
    let t_a = insert_turn(
        &pool,
        TurnSpec {
            session_id: session_pk_a,
            project_id: project_id_a,
            start_request_id: r_a_id,
            closed_at: Some(now + ChronoDuration::milliseconds(20)),
            embedded_at: None,
        },
    )
    .await?;
    let t_b = insert_turn(
        &pool,
        TurnSpec {
            session_id: session_pk_b,
            project_id: project_id_b,
            start_request_id: r_b_id,
            closed_at: Some(now + ChronoDuration::milliseconds(30)),
            embedded_at: None,
        },
    )
    .await?;

    let mut tx1 = pool.begin().await?;
    let locked: (i64,) = sqlx::query_as(
        "SELECT id
           FROM memory.operator_turns
          WHERE id = $1
          FOR UPDATE",
    )
    .bind(t_a)
    .fetch_one(&mut *tx1)
    .await?;
    assert_eq!(locked.0, t_a, "tx1 must lock t_a; got {} instead", locked.0);

    let mut tx2 = pool.begin().await?;
    let visible: Vec<(i64, String, String)> = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        sqlx::query_as::<_, (i64, String, String)>(EMBED_WORKER_SELECT)
            .bind(16i64)
            .fetch_all(&mut *tx2),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "embed worker SELECT blocked on locked row — `SKIP LOCKED` must be in effect, \
             but the SELECT did not return within 2s"
        )
    })??;
    let visible_ids: Vec<i64> = visible.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(
        visible_ids,
        vec![t_b],
        "tx2 must see only the un-locked candidate (t_b={t_b}); got {visible_ids:?}. \
         t_a={t_a} is held by tx1 and must be skipped, not blocked-on."
    );
    tx2.commit().await?;

    tx1.commit().await?;

    let after_release: Vec<(i64, String, String)> = sqlx::query_as(EMBED_WORKER_SELECT)
        .bind(16i64)
        .fetch_all(&pool)
        .await?;
    let after_ids: Vec<i64> = after_release.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(
        after_ids,
        vec![t_a, t_b],
        "after tx1 commits, both candidates must be visible in closed_at ASC order; got {after_ids:?}"
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
