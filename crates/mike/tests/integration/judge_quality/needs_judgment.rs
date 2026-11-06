use super::fixtures::{
    bootstrap_project_session, resolve_outcome_id, seed_completed_request,
    seed_session_with_last_seen, seed_unjudged_operator_turn,
};

use crate::test_db;

use chrono::Utc;
use mike::store::persistence::{
    NeedsJudgmentQuery, SessionForJudgment, fetch_sessions_needing_judgment,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;
use uuid::Uuid;

async fn select_sessions(
    pool: &sqlx::PgPool,
    judge_model: &str,
    rubric_version: i32,
    settled_before: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<Vec<SessionForJudgment>> {
    let query = NeedsJudgmentQuery {
        judge_model,
        rubric_version,
        settled_before,
        limit: 100,
        max_attempts_per_day: 3,
    };
    Ok(fetch_sessions_needing_judgment(pool, &query).await?)
}

#[tokio::test]
async fn sessions_needing_overview_are_selected_without_needing_a_verdict() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let model = "test-judge/overview";
    let rubric: i32 = 1;
    let settled_before = Utc::now() + chrono::Duration::hours(1);
    let outcome_id = resolve_outcome_id(&pool, "success").await?;

    async fn give_verdict(
        pool: &PgPool,
        session_pk: i64,
        outcome_id: i64,
        model: &str,
        rubric: i32,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO session_judges.verdicts
                 (session_id, outcome_id, quality_score, rationale, judge_model, rubric_version)
             VALUES ($1, $2, 0.9::numeric(4,3), 'good', $3, $4)",
        )
        .bind(session_pk)
        .bind(outcome_id)
        .bind(model)
        .bind(rubric)
        .execute(pool)
        .await?;
        Ok(())
    }

    let (pid_a, pk_a, _) =
        bootstrap_project_session(&pool, &format!("/test/ov-a/{}", Uuid::new_v4().simple()))
            .await?;
    seed_completed_request(&pool, pk_a, pid_a).await?;
    seed_unjudged_operator_turn(&pool, pk_a, pid_a, Utc::now()).await?;
    give_verdict(&pool, pk_a, outcome_id, model, rubric).await?;

    let (pid_b, pk_b, _) =
        bootstrap_project_session(&pool, &format!("/test/ov-b/{}", Uuid::new_v4().simple()))
            .await?;
    seed_completed_request(&pool, pk_b, pid_b).await?;
    seed_unjudged_operator_turn(&pool, pk_b, pid_b, Utc::now()).await?;
    give_verdict(&pool, pk_b, outcome_id, model, rubric).await?;
    sqlx::query("UPDATE sessions.sessions SET overview = 'Already written.' WHERE id = $1")
        .bind(pk_b)
        .execute(&pool)
        .await?;

    let (pid_c, pk_c, _) =
        bootstrap_project_session(&pool, &format!("/test/ov-c/{}", Uuid::new_v4().simple()))
            .await?;
    seed_completed_request(&pool, pk_c, pid_c).await?;
    give_verdict(&pool, pk_c, outcome_id, model, rubric).await?;

    let rows = select_sessions(&pool, model, rubric, settled_before).await?;
    let row_a = rows.iter().find(|r| r.session_db_id == pk_a);

    let Some(row_a) = row_a else {
        panic!(
            "a judged session missing its overview must be selected so the overview can backfill"
        );
    };
    assert!(
        !row_a.needs_verdict,
        "an overview-only pass must report needs_verdict = false so the standing verdict survives"
    );
    assert!(
        !rows.iter().any(|r| r.session_db_id == pk_b),
        "a session with both a verdict and an overview must not be selected"
    );
    assert!(
        !rows.iter().any(|r| r.session_db_id == pk_c),
        "a judged session with no operator turns must not be re-selected for an overview"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn stale_unknown_verdict_is_rescored_once_turns_land() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let model = "test-judge/stale-unknown";
    let rubric: i32 = 1;
    let settled_before = Utc::now() + chrono::Duration::hours(1);
    let unknown_id = resolve_outcome_id(&pool, "unknown").await?;

    async fn give_unknown_verdict(
        pool: &PgPool,
        session_pk: i64,
        outcome_id: i64,
        model: &str,
        rubric: i32,
        judged_at: chrono::DateTime<Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO session_judges.verdicts
                 (session_id, outcome_id, quality_score, rationale, judge_model,
                  rubric_version, judged_at)
             VALUES ($1, $2, 0.0::numeric(4,3),
                     'The session had no operator turns.', $3, $4, $5)",
        )
        .bind(session_pk)
        .bind(outcome_id)
        .bind(model)
        .bind(rubric)
        .bind(judged_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    let (pid_a, pk_a, _) =
        bootstrap_project_session(&pool, &format!("/test/stale-a/{}", Uuid::new_v4().simple()))
            .await?;
    seed_completed_request(&pool, pk_a, pid_a).await?;
    give_unknown_verdict(
        &pool,
        pk_a,
        unknown_id,
        model,
        rubric,
        Utc::now() - chrono::Duration::hours(1),
    )
    .await?;
    seed_unjudged_operator_turn(&pool, pk_a, pid_a, Utc::now()).await?;

    let (pid_b, pk_b, _) =
        bootstrap_project_session(&pool, &format!("/test/stale-b/{}", Uuid::new_v4().simple()))
            .await?;
    seed_completed_request(&pool, pk_b, pid_b).await?;
    seed_unjudged_operator_turn(&pool, pk_b, pid_b, Utc::now()).await?;
    give_unknown_verdict(
        &pool,
        pk_b,
        unknown_id,
        model,
        rubric,
        Utc::now() + chrono::Duration::minutes(1),
    )
    .await?;

    let rows = select_sessions(&pool, model, rubric, settled_before).await?;

    let Some(row_a) = rows.iter().find(|r| r.session_db_id == pk_a) else {
        panic!("a stale `unknown` verdict with a newer operator turn must be re-selected");
    };
    assert!(
        row_a.needs_verdict,
        "a stale `unknown` verdict must be re-scored, not left standing"
    );

    let row_b = rows.iter().find(|r| r.session_db_id == pk_b);
    assert!(
        row_b.is_none_or(|r| !r.needs_verdict),
        "an `unknown` verdict newer than every turn must not be re-scored — \
         that is what stops the poll re-judging the same session forever"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn sessions_needing_judgment_excludes_already_judged() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let model = "test-judge/v0";
    let rubric: i32 = 1;
    let settled_before = Utc::now() + chrono::Duration::hours(1);

    let project_a = format!("/test/judge-a/{}", Uuid::new_v4().simple());
    let (project_id_a, session_pk_a, _uuid_a) =
        bootstrap_project_session(&pool, &project_a).await?;
    seed_unjudged_operator_turn(&pool, session_pk_a, project_id_a, Utc::now()).await?;
    sqlx::query("UPDATE sessions.sessions SET overview = 'Already written.' WHERE id = $1")
        .bind(session_pk_a)
        .execute(&pool)
        .await?;
    let outcome_id = resolve_outcome_id(&pool, "success").await?;
    sqlx::query(
        "INSERT INTO session_judges.verdicts
             (session_id, outcome_id, quality_score, rationale, judge_model, rubric_version)
         VALUES ($1, $2, 0.9::numeric(4,3), 'good', $3, $4)",
    )
    .bind(session_pk_a)
    .bind(outcome_id)
    .bind(model)
    .bind(rubric)
    .execute(&pool)
    .await?;

    let project_b = format!("/test/judge-b/{}", Uuid::new_v4().simple());
    let (project_id_b, session_pk_b, _uuid_b) =
        bootstrap_project_session(&pool, &project_b).await?;
    seed_unjudged_operator_turn(&pool, session_pk_b, project_id_b, Utc::now()).await?;

    let project_c = format!("/test/judge-c/{}", Uuid::new_v4().simple());
    let (project_id_c, session_pk_c, _uuid_c) =
        bootstrap_project_session(&pool, &project_c).await?;
    seed_completed_request(&pool, session_pk_c, project_id_c).await?;

    let rows = select_sessions(&pool, model, rubric, settled_before).await?;
    let found_pks: Vec<i64> = rows.iter().map(|row| row.session_db_id).collect();

    assert!(
        !found_pks.contains(&session_pk_a),
        "session A (already judged) must not appear in the needs-judgment list"
    );
    assert!(
        found_pks.contains(&session_pk_b),
        "session B (not yet judged) must appear in the needs-judgment list"
    );
    assert!(
        !found_pks.contains(&session_pk_c),
        "session C (requests but no operator turn) must not be judged — a session's \
         opening requests are machine traffic, so there is nothing to read yet"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn sessions_needing_judgment_orders_newest_first() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let model = "test-judge/order-v0";
    let rubric: i32 = 1;
    let settled_before = Utc::now() + chrono::Duration::hours(1);
    let base = Utc::now() - chrono::Duration::hours(2);

    let last_seen_oldest = base;
    let last_seen_middle = base + chrono::Duration::minutes(20);
    let last_seen_newest = base + chrono::Duration::minutes(40);

    let (_pk_oldest, uuid_oldest) = seed_session_with_last_seen(
        &pool,
        &format!("/test/judge-order-a/{}", Uuid::new_v4().simple()),
        last_seen_oldest,
    )
    .await?;
    let (_pk_newest, uuid_newest) = seed_session_with_last_seen(
        &pool,
        &format!("/test/judge-order-b/{}", Uuid::new_v4().simple()),
        last_seen_newest,
    )
    .await?;
    let (_pk_middle, uuid_middle) = seed_session_with_last_seen(
        &pool,
        &format!("/test/judge-order-c/{}", Uuid::new_v4().simple()),
        last_seen_middle,
    )
    .await?;

    let rows = select_sessions(&pool, model, rubric, settled_before).await?;
    let found: Vec<String> = rows
        .into_iter()
        .map(|row| row.session_id)
        .filter(|id| [&uuid_oldest, &uuid_middle, &uuid_newest].contains(&id))
        .collect();

    assert_eq!(
        found,
        vec![uuid_newest, uuid_middle, uuid_oldest],
        "fetch_sessions_needing_judgment must return newest-last_seen-first"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn sessions_needing_judgment_respects_limit() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let model = "test-judge/limit-v0";
    let rubric: i32 = 1;
    let settled_before = Utc::now() + chrono::Duration::hours(1);
    let base = Utc::now() - chrono::Duration::hours(1);

    for i in 0..5i64 {
        seed_session_with_last_seen(
            &pool,
            &format!("/test/judge-limit-{i}/{}", Uuid::new_v4().simple()),
            base + chrono::Duration::minutes(i),
        )
        .await?;
    }

    let query = NeedsJudgmentQuery {
        judge_model: model,
        rubric_version: rubric,
        settled_before,
        limit: 2,
        max_attempts_per_day: 3,
    };
    let rows = fetch_sessions_needing_judgment(&pool, &query).await?;
    assert_eq!(
        rows.len(),
        2,
        "limit=2 must cap the result to 2 rows regardless of how many sessions qualify"
    );

    handle.cleanup().await?;
    Ok(())
}
