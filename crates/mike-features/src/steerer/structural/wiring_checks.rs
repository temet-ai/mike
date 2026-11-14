use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;

use super::never_fired::{FLEET_EVENT_KIND, check_never_written_fleet_event_kind};
use super::wiring::{
    CoverageRow, FLEET_EVENT_WINDOW_DAYS, Finding, GRADE_MIN_EVENTS, JobRow, RetirementRow,
    SEARCH_GRADE_COVERAGE, STAGE_SAMPLE_REQUESTS, SUPPLY_VERDICT_COVERAGE, ShadowQuestionRow,
    StageRow, SteererActivityRow, ToggleRow, UNAUDITED_FLEET_DAYS, UnauditedFleet,
    VERDICT_MIN_ITEMS, coverage_findings, dormant_findings, fleet_event_findings, job_findings,
    retirement_findings, shadow_findings, stage_findings, toggle_findings,
    unaudited_fleet_findings,
};

type Checked = Result<Option<Vec<Finding>>, sqlx::Error>;

pub(super) async fn stage_applied(pool: &PgPool) -> Checked {
    let rows = sqlx::query_as!(
        StageRow,
        r#"
        WITH recent AS (
            SELECT transformation_manifest AS manifest
              FROM requests.requests
             WHERE created_at > now() - interval '24 hours'
               AND transformation_manifest IS NOT NULL
             ORDER BY created_at DESC
             LIMIT $1
        ),
        staged AS (
            SELECT st ->> 'stage' AS stage,
                   st ->> 'outcome' AS outcome,
                   st ->> 'reason_code' AS reason
              FROM recent
             CROSS JOIN LATERAL jsonb_array_elements(manifest -> 'stages') AS st
        )
        SELECT stage AS "stage!",
               count(*) AS "seen!",
               count(*) FILTER (WHERE outcome IS DISTINCT FROM 'unchanged') AS "applied!",
               mode() WITHIN GROUP (ORDER BY reason) AS top_reason
          FROM staged
         WHERE stage IS NOT NULL
         GROUP BY stage
        "#,
        STAGE_SAMPLE_REQUESTS
    )
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(stage_findings(&rows)))
}

pub(super) async fn fleet_events_written(pool: &PgPool) -> Checked {
    let finding =
        check_never_written_fleet_event_kind(pool, &FLEET_EVENT_KIND, FLEET_EVENT_WINDOW_DAYS)
            .await?;
    Ok(fleet_event_findings(&finding))
}

pub(super) async fn toggles_off(pool: &PgPool, now: DateTime<Utc>) -> Checked {
    let rows = sqlx::query_as!(
        ToggleRow,
        r#"
        SELECT key AS "key!", operator_set AS "operator_set!", updated_at AS "updated_at!"
          FROM config.feature_toggles
         WHERE NOT enabled
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(Some(toggle_findings(&rows, now)))
}

pub(super) async fn steerer_state(pool: &PgPool, now: DateTime<Utc>) -> Checked {
    let questions = sqlx::query_as!(
        ShadowQuestionRow,
        r#"
        SELECT s.name AS "steerer!", q.question AS "question!", q.updated_at AS "updated_at!"
          FROM steering.question_policies q
          JOIN steering.steerers s ON s.id = q.steerer_id
         WHERE q.mode = 'shadow' AND s.deleted_at IS NULL
        "#
    )
    .fetch_all(pool)
    .await?;
    let steerers = sqlx::query_as!(
        SteererActivityRow,
        r#"
        SELECT s.name AS "name!",
               count(*) FILTER (WHERE e.mode = 'shadow') AS "shadow!",
               count(*) FILTER (WHERE e.mode = 'live') AS "live!",
               min(e.dispatched_at) FILTER (WHERE e.mode = 'shadow') AS first_shadow,
               max(e.dispatched_at) AS last_evaluation
          FROM steering.steerers s
          JOIN steering.evaluations e ON e.steerer_id = s.id
         WHERE s.deleted_at IS NULL
         GROUP BY s.name
        "#
    )
    .fetch_all(pool)
    .await?;
    let mut findings = shadow_findings(&questions, &steerers, now);
    findings.extend(dormant_findings(&steerers, now));
    Ok(Some(findings))
}

pub(super) async fn jobs_not_succeeding(pool: &PgPool, now: DateTime<Utc>) -> Checked {
    let rows = sqlx::query_as!(
        JobRow,
        r#"
        SELECT job_name AS "job_name!",
               max(finished_at) FILTER (WHERE result_kind = 'succeeded') AS last_success,
               max(finished_at) AS "last_run!",
               (array_agg(result_kind ORDER BY finished_at DESC))[1] AS last_result
          FROM jobs.runs
         GROUP BY job_name
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(Some(job_findings(&rows, now)))
}

pub(super) async fn closed_fleets_unaudited(pool: &PgPool) -> Checked {
    let rows = sqlx::query_as!(
        UnauditedFleet,
        r#"
        SELECT f.slug AS "slug!", f.finished_at AS "finished_at!"
          FROM launch.fleets f
         WHERE f.state = 'done'
           AND f.run_id IS NOT NULL
           AND f.finished_at > now() - make_interval(days => $1)
           AND f.finished_at < now() - interval '2 hours'
           AND NOT EXISTS (
                SELECT 1 FROM launch.runs r WHERE r.run_id = 'rule-audit-' || f.run_id
           )
         ORDER BY f.finished_at DESC
        "#,
        UNAUDITED_FLEET_DAYS
    )
    .fetch_all(pool)
    .await?;
    Ok(Some(unaudited_fleet_findings(&rows)))
}

pub(super) async fn supply_verdict_coverage(pool: &PgPool, now: DateTime<Utc>) -> Checked {
    let row = sqlx::query_as!(
        CoverageRow,
        r#"
        SELECT count(*) AS "total!", count(v.supplied_item_id) AS "covered!"
          FROM memory.supplied_items i
          LEFT JOIN memory.supplied_item_verdicts v ON v.supplied_item_id = i.id
         WHERE i.supplied_at BETWEEN $1 AND $2
        "#,
        now - Duration::hours(72),
        now - Duration::hours(24)
    )
    .fetch_one(pool)
    .await?;
    Ok(coverage_findings(
        SUPPLY_VERDICT_COVERAGE,
        "supplied_items",
        "memory items supplied 24 to 72 hours ago have a verdict",
        row,
        VERDICT_MIN_ITEMS,
    ))
}

pub(super) async fn search_grade_coverage(pool: &PgPool, now: DateTime<Utc>) -> Checked {
    let row = sqlx::query_as!(
        CoverageRow,
        r#"
        SELECT count(*) AS "total!", count(g.event_id) AS "covered!"
          FROM memory.search_events e
          LEFT JOIN memory.search_grades g ON g.event_id = e.id
         WHERE e.created_at BETWEEN $1 AND $2
        "#,
        now - Duration::hours(72),
        now - Duration::hours(24)
    )
    .fetch_one(pool)
    .await?;
    Ok(coverage_findings(
        SEARCH_GRADE_COVERAGE,
        "search_events",
        "searches from 24 to 72 hours ago have a grade",
        row,
        GRADE_MIN_EVENTS,
    ))
}

pub(super) async fn lesson_retirement(pool: &PgPool) -> Checked {
    let row = sqlx::query_as!(
        RetirementRow,
        r#"
        SELECT count(*) AS "lessons!", count(retired_at) AS "retired!"
          FROM memory.lessons
        "#
    )
    .fetch_one(pool)
    .await?;
    Ok(retirement_findings(row))
}
