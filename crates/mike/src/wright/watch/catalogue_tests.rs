use chrono::{Duration, Utc};

use super::daily_pass_due;
use super::fixtures_tests::Db;
use crate::store::persistence::wright::NewCheck;
use crate::wright::{Probe, Source};

fn codes(cadences: &[&str]) -> Vec<String> {
    cadences.iter().map(|c| (*c).to_owned()).collect()
}

async fn due_names(db: &Db, cadences: &[&str]) -> anyhow::Result<Vec<String>> {
    Ok(db
        .handle
        .enabled_wright_checks(&codes(cadences))
        .await?
        .into_iter()
        .map(|c| c.name)
        .collect())
}

#[tokio::test]
async fn every_source_has_at_least_one_seeded_check() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let checks = db.handle.list_wright_checks().await?;
    for source in Source::ALL {
        assert!(
            checks.iter().any(|c| c.source == source.code()),
            "no check reads {}",
            source.code()
        );
    }
    db.cleanup().await
}

#[tokio::test]
async fn every_seeded_query_is_a_bounded_read_with_its_window_in_params() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    for check in db.handle.list_wright_checks().await? {
        if Probe::from_code(&check.probe) != Some(Probe::Sql) {
            assert!(check.query.is_none(), "{} must hold no query", check.name);
            continue;
        }
        let sql = check.query.unwrap_or_default();
        let trimmed = sql.trim_start().to_ascii_uppercase();
        assert!(
            trimmed.starts_with("WITH") || trimmed.starts_with("SELECT"),
            "{} must be a read",
            check.name
        );
        assert!(sql.contains("LIMIT 200"), "{} must be bounded", check.name);
        assert!(
            sql.contains("make_interval") && sql.contains("$1"),
            "{} must read its window from params",
            check.name
        );
    }
    db.cleanup().await
}

#[tokio::test]
async fn due_runs_event_checks_on_the_regular_pass_and_slow_checks_daily() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let regular = due_names(&db, &["event", "regular"]).await?;
    assert!(regular.contains(&"request_failures".to_owned()));
    assert!(regular.contains(&"log_warnings".to_owned()));
    assert!(!regular.contains(&"cold_forks".to_owned()));
    assert_eq!(due_names(&db, &["event"]).await?, ["request_failures"]);
    assert_eq!(
        due_names(&db, &["daily"]).await?,
        ["cold_forks", "scheduled_job_failures"]
    );
    db.cleanup().await
}

#[tokio::test]
async fn a_disabled_check_is_not_due_and_a_new_check_is() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let Some(row) = db.handle.wright_check("request_failures").await? else {
        anyhow::bail!("the seed must hold request_failures");
    };
    let mut off = NewCheck::from(row);
    off.enabled = false;
    db.handle.replace_wright_check(&off).await?;
    assert!(due_names(&db, &["event"]).await?.is_empty());

    let mut added = off.clone();
    added.name = "extra_failures".to_owned();
    added.enabled = true;
    db.handle.insert_wright_check(&added).await?;
    assert_eq!(due_names(&db, &["event"]).await?, ["extra_failures"]);
    db.cleanup().await
}

#[test]
fn daily_pass_due_once_per_utc_day() {
    let now = Utc::now();
    assert!(daily_pass_due(None, now));
    assert!(!daily_pass_due(Some(now), now));
    assert!(daily_pass_due(Some(now - Duration::days(1)), now));
}
