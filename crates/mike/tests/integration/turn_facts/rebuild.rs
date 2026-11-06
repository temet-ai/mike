use chrono::{Duration, Utc};
use mike::config::subcommands::dossier_backfill::{BackfillOptions, BackfillStage, guard_execute};
use mike::dossier::build::build_sessions;

use super::resolution::{build, cfg, facts, message};
use super::{Fixture, fixture, sample_row};

async fn seed_pair(f: &Fixture) -> anyhow::Result<(i64, i64)> {
    let first = message(
        f,
        9100,
        0,
        "please add a retry to the http client",
        Some(100),
    )
    .await?;
    let second = message(f, 9101, 100, "and then add the docs", Some(200)).await?;
    Ok((first, second))
}

#[tokio::test]
async fn rebuild_with_no_change_writes_no_rows() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_pair(&f).await?;
    let first = build(&f).await?;
    assert_eq!(first.inserted, 2, "first: {first:?}");
    let second = build(&f).await?;
    assert_eq!(
        (second.inserted, second.updated),
        (0, 0),
        "an unchanged rebuild must upsert nothing: {second:?}"
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn no_op_build_moves_built_at_forward() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_pair(&f).await?;
    let first_at = Utc::now();
    build_sessions(&f.handle, &[f.session_id], first_at, &cfg()).await?;
    let before = facts(&f).await?;
    let later = first_at + Duration::minutes(3);
    let report = build_sessions(&f.handle, &[f.session_id], later, &cfg()).await?;
    assert_eq!(report.rows_written(), 0, "report: {report:?}");
    assert_eq!(report.stamped, 2, "report: {report:?}");
    let after = facts(&f).await?;
    for row in &after {
        let old = before
            .iter()
            .find(|b| b.operator_turn_id == row.operator_turn_id)
            .ok_or_else(|| anyhow::anyhow!("row {} vanished", row.operator_turn_id))?;
        assert!(
            row.built_at > old.built_at,
            "turn {}: built_at {} did not move past {}",
            row.operator_turn_id,
            row.built_at,
            old.built_at
        );
    }
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn older_build_does_not_overwrite_a_newer_row() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (turn, _) = seed_pair(&f).await?;
    let mut newer = sample_row(turn, f.session_id, f.base + Duration::minutes(10));
    newer.text_chars = 42;
    f.handle
        .upsert_turn_facts(std::slice::from_ref(&newer))
        .await?;
    let mut older = sample_row(turn, f.session_id, f.base + Duration::minutes(1));
    older.text_chars = 7;
    let outcome = f.handle.upsert_turn_facts(&[older]).await?;
    assert_eq!(
        (outcome.inserted, outcome.updated),
        (0, 0),
        "an older build must not win: {outcome:?}"
    );
    let stored = facts(&f).await?;
    let row = stored
        .iter()
        .find(|row| row.operator_turn_id == turn)
        .ok_or_else(|| anyhow::anyhow!("no row for turn {turn}"))?;
    assert_eq!(row.text_chars, 42);
    assert_eq!(row.built_at, newer.built_at);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn facts_stage_execute_refuses_while_agent_id_primary_requests_remain() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed_pair(&f).await?;
    sqlx::query(
        "UPDATE requests.requests
            SET claude_code_agent_id = 'agent-leftover', created_at = now()
          WHERE id = 9100",
    )
    .execute(f.handle.pool())
    .await?;
    let opts = BackfillOptions {
        from: f.base - Duration::days(1),
        until: None,
        stage: BackfillStage::Facts,
        batch_sessions: 50,
        execute: true,
    };
    let refused = guard_execute(&f.handle, &opts).await;
    assert!(
        refused.is_err(),
        "--stage facts --execute must refuse while an agent-id primary request remains"
    );
    let dry = BackfillOptions {
        execute: false,
        ..opts
    };
    assert!(guard_execute(&f.handle, &dry).await.is_ok());
    f.db.cleanup().await?;
    Ok(())
}
