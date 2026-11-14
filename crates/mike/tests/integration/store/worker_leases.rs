use std::time::Duration;

use mike::llm::LlmRole;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::llm_calls::LlmCallInsert;
use sqlx::PgPool;
use uuid::Uuid;

use crate::test_db::TestDb;

const TIMEOUT: Duration = Duration::from_secs(180);

async fn age_lease(pool: &PgPool, role: &str, seconds: i32) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE llm.worker_leases
            SET heartbeat_at = now() - make_interval(secs => $2)
          WHERE role = $1",
    )
    .bind(role)
    .bind(seconds)
    .execute(pool)
    .await?;
    Ok(())
}

async fn holder_of(pool: &PgPool, role: &str) -> anyhow::Result<Uuid> {
    Ok(
        sqlx::query_scalar("SELECT machine_uuid FROM llm.worker_leases WHERE role = $1")
            .bind(role)
            .fetch_one(pool)
            .await?,
    )
}

#[tokio::test]
async fn claim_first_machine_takes_the_role_and_renews_it() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let machine = Uuid::new_v4();

    assert!(
        handle
            .claim_llm_worker_lease("steerer", machine, TIMEOUT)
            .await?
    );
    age_lease(handle.pool(), "steerer", 100).await?;
    assert!(
        handle
            .claim_llm_worker_lease("steerer", machine, TIMEOUT)
            .await?
    );

    let leases = handle.list_llm_worker_leases().await?;
    assert_eq!(leases.len(), 1);
    assert_eq!(leases[0].role, "steerer");
    assert_eq!(leases[0].machine_uuid, machine);
    assert!(
        leases[0].age_secs < 30.0,
        "renewal must reset the heartbeat, got age {}",
        leases[0].age_secs
    );

    handle.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn claim_other_machine_is_refused_while_the_heartbeat_is_fresh() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (owner, rival) = (Uuid::new_v4(), Uuid::new_v4());

    assert!(
        handle
            .claim_llm_worker_lease("session_judge", owner, TIMEOUT)
            .await?
    );
    age_lease(handle.pool(), "session_judge", 170).await?;
    assert!(
        !handle
            .claim_llm_worker_lease("session_judge", rival, TIMEOUT)
            .await?
    );
    assert_eq!(holder_of(handle.pool(), "session_judge").await?, owner);
    assert!(
        handle
            .claim_llm_worker_lease("eval", rival, TIMEOUT)
            .await?,
        "a different role is independent"
    );

    handle.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn claim_other_machine_takes_over_a_stale_lease() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (owner, rival) = (Uuid::new_v4(), Uuid::new_v4());

    assert!(
        handle
            .claim_llm_worker_lease("judge_worker", owner, TIMEOUT)
            .await?
    );
    age_lease(handle.pool(), "judge_worker", 190).await?;
    assert!(
        handle
            .claim_llm_worker_lease("judge_worker", rival, TIMEOUT)
            .await?
    );
    assert_eq!(holder_of(handle.pool(), "judge_worker").await?, rival);
    assert!(
        !handle
            .claim_llm_worker_lease("judge_worker", owner, TIMEOUT)
            .await?,
        "the previous holder is refused once the new holder's heartbeat is fresh"
    );

    handle.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn claim_two_machines_at_once_gives_the_role_to_exactly_one() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let (first, second) = tokio::join!(
        handle.claim_llm_worker_lease("lesson_proposer", Uuid::new_v4(), TIMEOUT),
        handle.claim_llm_worker_lease("lesson_proposer", Uuid::new_v4(), TIMEOUT),
    );
    assert_eq!(
        [first?, second?].iter().filter(|held| **held).count(),
        1,
        "exactly one machine may hold the role"
    );

    handle.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn claim_on_a_closed_pool_returns_an_error() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    handle.pool().close().await;

    assert!(
        handle
            .claim_llm_worker_lease("steerer", Uuid::new_v4(), TIMEOUT)
            .await
            .is_err()
    );
    assert!(handle.list_llm_worker_leases().await.is_err());
    assert!(
        !handle
            .clone()
            .with_llm_machine(Uuid::new_v4())
            .holds_llm_role(LlmRole::Steerer, Duration::ZERO)
            .await,
        "a proxy that cannot reach the database must not run the role"
    );
    db.cleanup().await
}

#[tokio::test]
async fn holds_llm_role_only_the_first_machine_runs_a_role() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let base = PersistHandle::connect(&db.database_url).await?;
    let mac = base.clone().with_llm_machine(Uuid::new_v4());
    let omit = base.clone().with_llm_machine(Uuid::new_v4());

    assert!(mac.holds_llm_role(LlmRole::Steerer, Duration::ZERO).await);
    assert!(!omit.holds_llm_role(LlmRole::Steerer, Duration::ZERO).await);
    assert!(
        omit.holds_llm_role(LlmRole::Eval, Duration::ZERO).await,
        "each role has its own owner"
    );
    assert!(
        omit.holds_llm_role(LlmRole::Unattributed, Duration::ZERO)
            .await
    );
    assert!(
        mac.holds_llm_role(LlmRole::Steerer, Duration::ZERO).await,
        "a repeat check within the renewal period keeps the answer"
    );

    base.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn insert_llm_call_records_the_machine_that_made_the_call() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let machine = Uuid::new_v4();
    let handle = PersistHandle::connect(&db.database_url)
        .await?
        .with_llm_machine(machine);

    let id = handle
        .insert_llm_call(LlmCallInsert {
            role: "steerer",
            model: "test/model",
            generation_id: None,
            prompt_tokens: None,
            completion_tokens: None,
            reasoning_tokens: None,
            cached_tokens: None,
            cost_usd: None,
            latency_ms: None,
            outcome: "ok",
            http_status: None,
            session_id: None,
            steerer_id: None,
            operator_turn_id: None,
            request_body: None,
            response_body: None,
        })
        .await?;
    let older: i64 = sqlx::query_scalar(
        "INSERT INTO llm.calls (role, model, outcome) VALUES ('steerer', 'm', 'ok') RETURNING id",
    )
    .fetch_one(handle.pool())
    .await?;

    let recorded: Option<Uuid> =
        sqlx::query_scalar("SELECT machine_uuid FROM llm.calls WHERE id = $1")
            .bind(id)
            .fetch_one(handle.pool())
            .await?;
    let legacy: Option<Uuid> =
        sqlx::query_scalar("SELECT machine_uuid FROM llm.calls WHERE id = $1")
            .bind(older)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(recorded, Some(machine));
    assert_eq!(legacy, None);

    handle.pool().close().await;
    db.cleanup().await
}
