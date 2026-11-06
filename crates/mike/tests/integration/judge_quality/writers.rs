use chrono::{Duration, Utc};

use super::promotion_support::{projected, set_legacy};
use crate::turn_facts::{Req, fixture, insert_request};

#[tokio::test]
async fn idle_reap_leaves_outcome_columns_untouched() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    insert_request(&f, &Req::primary(21400, 0, "open")).await?;
    let open: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns (session_id, project_id, start_request_id, end_request_id, user_prompt)
         VALUES ($1, $2, 21400, 21400, 'open') RETURNING id",
    )
    .bind(f.session_id)
    .bind(f.project_id)
    .fetch_one(f.handle.pool())
    .await?;
    set_legacy(&f, open, "corrected").await?;
    sqlx::query(
        "UPDATE sessions.sessions SET last_seen = now() - interval '3 hours' WHERE id = $1",
    )
    .bind(f.session_id)
    .execute(f.handle.pool())
    .await?;
    let reaped = f
        .handle
        .reap_idle_open_turns(std::time::Duration::from_secs(1800), Utc::now())
        .await?;
    assert!(reaped.contains(&open));
    assert_eq!(
        projected(&f, open).await?.outcome,
        "corrected",
        "the reap writes no label"
    );
    Ok(())
}

#[tokio::test]
async fn a_projected_corrected_turn_stays_corrected_after_the_builder_runs() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = super::promotion_support::closed_turn(&f, 24_001).await?;
    set_legacy(&f, turn, "corrected").await?;
    mike::dossier::build::build_sessions(
        &f.handle,
        &[f.session_id],
        Utc::now() + Duration::hours(1),
        &mike::dossier::build::BuildConfig {
            settle: Duration::minutes(30),
        },
    )
    .await?;
    assert_eq!(projected(&f, turn).await?.outcome, "corrected");
    Ok(())
}
