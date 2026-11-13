use chrono::{DateTime, Duration, TimeZone, Utc};
use mike::store::persistence::eval_repo::EvalPersist as _;

use mike::store::persistence::eval_metrics::alarm_judge_call_loop::loop_fires;

use crate::turn_facts::{Fixture, fixture};

async fn calls(f: &Fixture, day: DateTime<Utc>, n: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO llm.calls (role, model, outcome, session_id, created_at)
         SELECT 'session_judge', 'google/gemini-3.5-flash', 'ok', $1, $2 + (g * interval '1 minute')
           FROM generate_series(1, $3) g",
    )
    .bind(f.session_id)
    .bind(day)
    .bind(n)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

async fn verdict_rows(f: &Fixture, day: DateTime<Utc>, n: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO session_judges.verdicts (session_id, outcome_id, quality_score, rationale, judge_model,
                                              rubric_version, judged_at)
         SELECT $1, o.id, 0.5, 'ok', 'google/gemini-3.5-flash', 2, $2 + (g * interval '1 minute')
           FROM generate_series(1, $3) g CROSS JOIN session_judges.outcomes o
          WHERE o.code = 'success'",
    )
    .bind(f.session_id)
    .bind(day)
    .bind(n)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn judge_call_loop_fires_on_a_storm_day_and_not_on_a_normal_one() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let storm = Utc
        .with_ymd_and_hms(2026, 9, 5, 0, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("date"))?;
    let normal = Utc
        .with_ymd_and_hms(2026, 9, 22, 0, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("date"))?;
    calls(&f, storm, 240).await?;
    verdict_rows(&f, storm, 2).await?;
    calls(&f, normal, 60).await?;
    verdict_rows(&f, normal, 60).await?;
    let storm_row = f
        .handle
        .eval()
        .eval_session_judge_call_loop(storm + Duration::days(1), 3)
        .await?;
    assert_eq!(storm_row.ok_calls, 240);
    assert!(loop_fires(&storm_row, 50, 3.0), "{storm_row:?}");
    assert_eq!(storm_row.top_sessions[0]["calls"], 240);
    let normal_row = f
        .handle
        .eval()
        .eval_session_judge_call_loop(normal + Duration::days(1), 3)
        .await?;
    assert!(!loop_fires(&normal_row, 50, 3.0), "{normal_row:?}");
    Ok(())
}
