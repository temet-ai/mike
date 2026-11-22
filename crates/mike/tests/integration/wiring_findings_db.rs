use chrono::{Duration, Utc};
use mike::steerer::structural::wiring::{Finding, PassOutcome};
use mike::steerer::structural::wiring_store::{StoredFinding, Tally, apply, list_findings};

use crate::test_db::TestDb;

const KIND: &str = "toggle_off";
const SUBJECT: &str = "toggle:db_resync";

fn outcome(checked: bool, figure: Option<f64>) -> PassOutcome {
    PassOutcome {
        checked: if checked { vec![KIND] } else { Vec::new() },
        findings: figure
            .into_iter()
            .map(|value| Finding {
                check_kind: KIND,
                subject: SUBJECT.to_owned(),
                figure_value: value,
                figure_text: format!("off for {value} days"),
            })
            .collect(),
        failed: Vec::new(),
    }
}

async fn only_finding(pool: &sqlx::PgPool) -> anyhow::Result<StoredFinding> {
    let rows = list_findings(pool, Utc::now() - Duration::days(1)).await?;
    let [row] = <[StoredFinding; 1]>::try_from(rows)
        .map_err(|rows| anyhow::anyhow!("expected one finding, got {rows:?}"))?;
    Ok(row)
}

#[tokio::test]
async fn finding_stays_open_until_its_check_passes_and_reopens_when_it_fails_again()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;

    apply(&pool, &outcome(true, Some(20.0))).await?;
    let first = only_finding(&pool).await?;
    assert_eq!(first.resolved_at, None);
    assert_eq!(first.figure_value, 20.0);

    apply(&pool, &outcome(true, Some(20.0))).await?;
    let same_figure = only_finding(&pool).await?;
    assert_eq!(same_figure.first_seen, first.first_seen);
    assert_eq!(same_figure.figure_changed_at, first.figure_changed_at);
    assert!(same_figure.last_seen > first.last_seen);

    apply(&pool, &outcome(true, Some(21.0))).await?;
    let moved = only_finding(&pool).await?;
    assert_eq!(moved.first_seen, first.first_seen);
    assert!(moved.figure_changed_at > first.figure_changed_at);
    assert_eq!(moved.figure_value, 21.0);

    let tally = apply(&pool, &outcome(false, None)).await?;
    assert_eq!(tally.resolved, 0);
    assert_eq!(only_finding(&pool).await?.resolved_at, None);

    let tally = apply(&pool, &outcome(true, None)).await?;
    assert_eq!(tally.resolved, 1);
    let resolved = only_finding(&pool).await?;
    assert!(resolved.resolved_at.is_some());

    apply(&pool, &outcome(true, Some(1.0))).await?;
    let reopened = only_finding(&pool).await?;
    assert_eq!(reopened.resolved_at, None);
    assert!(reopened.first_seen > first.first_seen);

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn resolved_finding_older_than_the_window_is_not_listed() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;

    apply(&pool, &outcome(true, Some(20.0))).await?;
    apply(&pool, &outcome(true, None)).await?;
    sqlx::query("UPDATE steering.wiring_findings SET resolved_at = now() - interval '30 days'")
        .execute(&pool)
        .await?;

    let rows = list_findings(&pool, Utc::now() - Duration::days(7)).await?;
    assert!(rows.is_empty());

    db.cleanup().await?;
    Ok(())
}

fn finding(kind: &'static str, subject: &str) -> Finding {
    Finding {
        check_kind: kind,
        subject: subject.to_owned(),
        figure_value: 1.0,
        figure_text: "found".to_owned(),
    }
}

fn open_subjects(rows: &[StoredFinding]) -> Vec<(String, String)> {
    let mut open: Vec<(String, String)> = rows
        .iter()
        .filter(|row| row.resolved_at.is_none())
        .map(|row| (row.check_kind.clone(), row.subject.clone()))
        .collect();
    open.sort();
    open
}

#[tokio::test]
async fn pass_resolves_only_the_checked_kinds_subjects_that_stopped_failing() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;
    let other = "job_not_succeeding";

    let opened = apply(
        &pool,
        &PassOutcome {
            checked: vec![KIND, other],
            findings: vec![finding(KIND, "a"), finding(KIND, "b"), finding(other, "a")],
            failed: Vec::new(),
        },
    )
    .await?;
    assert_eq!(
        opened,
        Tally {
            upserted: 3,
            resolved: 0
        }
    );

    let still = apply(
        &pool,
        &PassOutcome {
            checked: vec![KIND],
            findings: vec![finding(KIND, "b")],
            failed: Vec::new(),
        },
    )
    .await?;
    assert_eq!(still.upserted, 1);
    assert_eq!(still.resolved, 1);
    let rows = list_findings(&pool, Utc::now() - Duration::days(1)).await?;
    assert_eq!(
        open_subjects(&rows),
        vec![
            (other.to_owned(), "a".to_owned()),
            (KIND.to_owned(), "b".to_owned()),
        ]
    );

    let cleared = apply(
        &pool,
        &PassOutcome {
            checked: vec![KIND, other],
            findings: Vec::new(),
            failed: Vec::new(),
        },
    )
    .await?;
    assert_eq!(cleared.upserted, 0);
    assert_eq!(cleared.resolved, 2);
    let rows = list_findings(&pool, Utc::now() - Duration::days(1)).await?;
    assert!(open_subjects(&rows).is_empty());

    db.cleanup().await?;
    Ok(())
}
