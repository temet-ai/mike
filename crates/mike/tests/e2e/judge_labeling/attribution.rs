use crate::db_helpers;

use sqlx::PgPool;
use wiremock::MockServer;

use super::support::{mount_judge_stub, seed_unjudged_turns, start_proxy_with_judge};

async fn llm_call_rows(pool: &PgPool, expected: usize) -> anyhow::Result<Vec<(i64, i64, i64)>> {
    let mut last = Vec::new();
    for _ in 0..100 {
        last = sqlx::query_as::<_, (i64, i64, i64)>(
            "SELECT c.operator_turn_id, c.session_id, t.session_id
               FROM llm.calls c
               JOIN memory.operator_turns t ON t.id = c.operator_turn_id
              WHERE c.role = 'judge_worker' AND c.outcome = 'ok'
                AND c.operator_turn_id IS NOT NULL AND c.session_id IS NOT NULL
              ORDER BY c.operator_turn_id",
        )
        .fetch_all(pool)
        .await?;
        if last.len() >= expected {
            return Ok(last);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    anyhow::bail!(
        "expected {expected} attributed judge_worker rows in llm.calls, saw {}: {last:?}",
        last.len()
    )
}

#[tokio::test]
async fn judge_sweep_attributes_llm_calls_to_turn_and_session_and_refusals_write_no_rows()
-> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let or_mock = MockServer::start().await;

    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping judge_sweep_attributes_llm_calls");
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = "a2345678-f001-4f01-8f01-f01f01f01f21";
    seed_unjudged_turns(&pool, "test-judge-attribution", session_id, 2).await?;
    mount_judge_stub(&or_mock).await;

    let (_proxy_url, shutdown, state) =
        start_proxy_with_judge(&upstream_mock.uri(), &test_db.database_url, &or_mock.uri()).await?;

    mike::llm::reset_global_for_tests();
    let labeled = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(labeled, 2, "both seeded turns must be labeled");

    let rows = llm_call_rows(&pool, 2).await?;
    assert_eq!(rows.len(), 2);
    for (turn_id, call_session, turn_session) in &rows {
        assert_eq!(
            call_session, turn_session,
            "llm.calls.session_id must be the session that owns operator turn {turn_id}"
        );
    }

    seed_unjudged_turns(&pool, "test-judge-attribution", session_id, 1).await?;
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM llm.calls")
        .fetch_one(&pool)
        .await?;
    mike::llm::exhaust_global_rate_for_tests();
    let refused_sweep = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(refused_sweep, 0);
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM llm.calls")
        .fetch_one(&pool)
        .await?;
    let refused_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM llm.calls WHERE outcome LIKE 'refused%'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        after, before,
        "a deferred sweep must not write llm.calls rows"
    );
    assert_eq!(
        refused_rows, 0,
        "refusals are not calls and must not be recorded"
    );

    mike::llm::reset_global_for_tests();
    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}
