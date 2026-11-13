use crate::db_helpers;

use mike::analysis::judge::JUDGE_RUBRIC_VERSION;
use wiremock::MockServer;

use super::support::{
    count_judged_turns, count_with_judge_model, mount_judge_stub, seed_unjudged_turns,
    start_proxy_with_judge,
};

#[tokio::test]
async fn judge_labels_unjudged_turns_and_skips_already_judged() -> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let or_mock = MockServer::start().await;

    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = "12345678-f001-4f01-8f01-f01f01f01f01";
    let project_path = "test-judge-labeling";

    mike::llm::reset_global_for_tests();

    seed_unjudged_turns(&pool, project_path, session_id, 10).await?;
    mount_judge_stub(&or_mock).await;

    let (_proxy_url, shutdown, state) =
        start_proxy_with_judge(&upstream_mock.uri(), &test_db.database_url, &or_mock.uri()).await?;

    let labeled = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        labeled, 10,
        "crit 1: run_judge_once must label all 10 unjudged turns, got {labeled}"
    );

    let judged = count_judged_turns(&pool, session_id).await?;
    assert_eq!(
        judged, 10,
        "crit 1: all 10 turns must have a non-unknown outcome after sweep, got {judged}"
    );

    let with_model = count_with_judge_model(&pool, session_id).await?;
    assert_eq!(
        with_model, 10,
        "crit 2: judge_model must be set on all 10 stamped turns, got {with_model}"
    );

    let rubric_ver: Option<String> = sqlx::query_scalar(
        "SELECT t.judge_rubric_version
           FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid
          LIMIT 1",
    )
    .bind(session_id)
    .fetch_optional(&pool)
    .await?
    .flatten();

    assert_eq!(
        rubric_ver.as_deref(),
        Some(JUDGE_RUBRIC_VERSION),
        "crit 2: judge_rubric_version must match JUDGE_RUBRIC_VERSION, got {:?}",
        rubric_ver
    );

    let second_sweep = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        second_sweep, 0,
        "crit 3: second sweep must return 0 (no new unjudged rows), got {second_sweep}"
    );

    sqlx::query(
        "UPDATE memory.operator_turns t
            SET judge_rubric_version = NULL,
                outcome_id = (SELECT id FROM memory.turn_outcomes WHERE code = 'unknown')
           FROM sessions.sessions s
          WHERE s.id = t.session_id
            AND s.session_uuid = $1::text::uuid
            AND t.id IN (
                SELECT t2.id
                  FROM memory.operator_turns t2
                  JOIN sessions.sessions s2 ON s2.id = t2.session_id
                 WHERE s2.session_uuid = $1::text::uuid
                 ORDER BY t2.id ASC
                 LIMIT 3
            )",
    )
    .bind(session_id)
    .execute(&pool)
    .await?;

    mike::llm::reset_global_for_tests();

    let third_sweep = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        third_sweep, 3,
        "crit 4: third sweep must re-label the 3 rows whose rubric_version was cleared, got {third_sweep}"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}
