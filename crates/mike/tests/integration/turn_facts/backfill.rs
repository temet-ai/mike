use chrono::Duration;
use mike::config::subcommands::dossier_backfill::{BackfillOptions, BackfillStage, run_backfill};

use super::resolution::{facts, message};
use super::{Fixture, fixture};

fn options(f: &Fixture, execute: bool) -> BackfillOptions {
    BackfillOptions {
        from: f.base - Duration::days(1),
        until: None,
        stage: BackfillStage::All,
        batch_sessions: 50,
        execute,
    }
}

async fn seed(f: &Fixture) -> anyhow::Result<()> {
    message(
        f,
        9100,
        0,
        "please add a retry to the http client",
        Some(100),
    )
    .await?;
    message(f, 9101, 100, "and then add the docs", Some(200)).await?;
    Ok(())
}

async fn classification_rows(f: &Fixture) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM prompt_kinds.classifications")
            .fetch_one(f.handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn dossier_backfill_dry_run_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed(&f).await?;
    let mut lines = Vec::new();
    let summary = run_backfill(&f.handle, &options(&f, false), |line| {
        lines.push(line.clone());
    })
    .await?;
    assert!(summary.dry_run);
    assert!(summary.classified >= 2, "summary: {summary:?}");
    assert!(summary.rows_written >= 2, "summary: {summary:?}");
    assert!(lines.iter().all(|line| line.dry_run));
    assert_eq!(classification_rows(&f).await?, 0);
    assert!(facts(&f).await?.is_empty());
    let opened_by: Vec<Option<String>> =
        sqlx::query_scalar("SELECT opened_by FROM memory.operator_turns")
            .fetch_all(f.handle.pool())
            .await?;
    assert!(opened_by.iter().all(Option::is_none));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn dossier_backfill_execute_is_idempotent() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed(&f).await?;
    let first = run_backfill(&f.handle, &options(&f, true), |_| {}).await?;
    assert!(!first.dry_run);
    assert!(first.classified >= 2, "first: {first:?}");
    assert_eq!(facts(&f).await?.len(), 2);
    assert!(classification_rows(&f).await? >= 2);
    let second = run_backfill(&f.handle, &options(&f, true), |_| {}).await?;
    assert_eq!(
        (
            second.classified,
            second.opened_by_filled,
            second.rows_written
        ),
        (0, 0, 0),
        "second: {second:?}"
    );
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn dossier_backfill_facts_stage_alone_skips_classification() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seed(&f).await?;
    let opts = BackfillOptions {
        stage: BackfillStage::Facts,
        ..options(&f, true)
    };
    let summary = run_backfill(&f.handle, &opts, |_| {}).await?;
    assert!(summary.rows_written >= 2, "summary: {summary:?}");
    assert_eq!(facts(&f).await?.len(), 2);
    f.db.cleanup().await?;
    Ok(())
}
