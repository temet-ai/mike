use mike::config::subcommands::dossier_backfill::{BackfillOptions, BackfillStage, run_backfill};

use super::entries;
use crate::turn_facts::fixture;
use crate::turn_facts::resolution::{build, message};

#[tokio::test]
async fn a_later_opposite_directive_supersedes_the_earlier_one() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    message(&f, 9100, 0, "fix the parser, then commit", Some(100)).await?;
    message(&f, 9101, 100, "actually, don't commit yet", Some(200)).await?;
    message(&f, 9102, 200, "thanks", Some(300)).await?;
    build(&f).await?;
    let rows = entries(&f).await?;
    let directives: Vec<_> = rows.iter().filter(|r| r.kind == "directive").collect();
    assert_eq!(directives.len(), 2, "rows: {rows:?}");
    assert_eq!(directives[0].data["polarity"], "allow");
    assert_eq!(directives[0].status, "superseded");
    assert_eq!(
        directives[0].closed_by.as_deref(),
        Some("requests.requests:9101")
    );
    assert_eq!(directives[1].status, "open");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn setting_off_stops_entries_but_not_ledger_links_and_on_catches_up() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    f.handle
        .upsert_setting("dossier.entries.enabled", &serde_json::json!(false))
        .await?;
    message(&f, 9100, 0, "please commit when green", Some(100)).await?;
    message(&f, 9101, 100, "and push it", Some(200)).await?;
    let off = build(&f).await?;
    assert_eq!(off.entries.entries_written, 0);
    assert!(entries(&f).await?.is_empty());
    f.handle
        .upsert_setting("dossier.entries.enabled", &serde_json::json!(true))
        .await?;
    sqlx::query("UPDATE memory.turn_facts SET builder_version = 'stale' WHERE session_id = $1")
        .bind(f.session_id)
        .execute(f.handle.pool())
        .await?;
    let on = build(&f).await?;
    assert!(on.entries.entries_written >= 2, "report: {on:?}");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_second_all_stage_backfill_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    message(&f, 9100, 0, "please commit when green", Some(100)).await?;
    message(&f, 9101, 100, "and push it", Some(200)).await?;
    let opts = BackfillOptions {
        from: f.base - chrono::Duration::days(1),
        until: None,
        stage: BackfillStage::All,
        batch_sessions: 50,
        execute: true,
    };
    run_backfill(&f.handle, &opts, |_| {}).await?;
    let second = run_backfill(&f.handle, &opts, |_| {}).await?;
    assert_eq!(
        (
            second.rows_written,
            second.rows_linked,
            second.entries_written,
            second.closes_written,
            second.classified,
            second.opened_by_filled
        ),
        (0, 0, 0, 0, 0, 0),
        "second: {second:?}"
    );
    f.db.cleanup().await?;
    Ok(())
}
