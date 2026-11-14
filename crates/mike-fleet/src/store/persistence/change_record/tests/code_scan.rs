use crate::store::persistence::fleet_repo::FleetPersist as _;
use anyhow::Context;
use chrono::{Duration, Utc};

use super::fixture;
use super::seed::{CHECKOUT, event, project, session};
use crate::change_record::ChangeKind;
use crate::store::persistence::change_record::code_scan::{NewCodeScan, NewFinding};
use mike_base::code_notes::Delivered;

fn finding(rule: &str, fingerprint: &str) -> NewFinding {
    NewFinding {
        rule_id: rule.to_owned(),
        severity: "warning".to_owned(),
        message: format!("{rule} message"),
        line: 3,
        col: 1,
        matched: "x".to_owned(),
        fix: None,
        fingerprint: fingerprint.to_owned(),
        rule_source: "builtin".to_owned(),
    }
}

fn scan_of(event_id: i64, present: &[&str], added: Vec<NewFinding>) -> NewCodeScan {
    NewCodeScan {
        change_event_id: event_id,
        checkout: CHECKOUT.to_owned(),
        path: "src/a.rs".to_owned(),
        language: Some("rust".to_owned()),
        rules_version: "v1".to_owned(),
        rules_run: 9,
        bytes: 100,
        duration_us: 1500,
        findings_after: i32::try_from(present.len()).unwrap_or(0),
        present: present.iter().map(|p| (*p).to_owned()).collect(),
        added,
        ..NewCodeScan::default()
    }
}

async fn deliver_one_of_two(
    handle: &crate::store::persistence::PersistHandle,
) -> anyhow::Result<()> {
    let roots = vec![CHECKOUT.to_owned()];
    let severities = vec!["warning".to_owned(), "error".to_owned()];
    let ready = handle
        .fleet()
        .code_findings_ready_to_post(&roots, 0, &severities)
        .await?;
    assert_eq!(ready.len(), 2);
    assert_eq!(
        handle
            .fleet()
            .code_findings_unnamed_floor(&roots, 0)
            .await?,
        None
    );
    let delivered = handle
        .fleet()
        .mark_code_findings_delivered(&[Delivered {
            finding_id: ready[0].id,
            request_id: Some(4242),
            via: mike_base::code_notes::VIA_REQUEST,
        }])
        .await?;
    assert_eq!(delivered, 1);
    let left = handle
        .fleet()
        .code_findings_ready_to_post(&roots, 0, &severities)
        .await?;
    assert_eq!(left.len(), 1);

    Ok(())
}

#[tokio::test]
async fn a_scan_records_findings_closes_fixed_ones_and_delivers_to_the_author() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = project(pool).await?;
    let author = session(pool, project_id, None).await?;
    let mut first = event(project_id, ChangeKind::Edit, &["src/a.rs"]);
    first.session_id = Some(author);
    let mut second = first.clone();
    second.at += Duration::seconds(1);
    let ids = f
        .handle
        .fleet()
        .insert_change_events(&[first, second])
        .await?;
    let [e1, e2] = ids.as_slice() else {
        anyhow::bail!("expected two events, got {ids:?}");
    };

    let pending = f.handle.fleet().code_scan_pending(CHECKOUT, 0, 10).await?;
    assert_eq!(
        pending.iter().map(|p| p.id).collect::<Vec<_>>(),
        vec![*e1, *e2]
    );

    let added = vec![
        finding("rust-unsafe", "fp-a"),
        finding("rust-temp-dir", "fp-b"),
    ];
    let first_scan = f
        .handle
        .fleet()
        .record_code_scan(&scan_of(*e1, &["fp-a", "fp-b"], added))
        .await?
        .context("first scan not recorded")?;
    assert_eq!(first_scan.fixed, 0);
    let again = f
        .handle
        .fleet()
        .record_code_scan(&scan_of(*e1, &[], Vec::new()))
        .await?;
    assert_eq!(again, None, "a second scan of one event is ignored");
    assert_eq!(f.handle.fleet().code_scan_cursor(CHECKOUT).await?, *e1);

    let since = Utc::now() - Duration::hours(1);
    let listed = f.handle.fleet().list_code_findings(since, None, 10).await?;
    assert!(
        listed.iter().all(|r| r.session_id.is_some()),
        "the author is read from the event"
    );
    deliver_one_of_two(&f.handle).await?;

    let second_scan = f
        .handle
        .fleet()
        .record_code_scan(&scan_of(*e2, &["fp-b"], Vec::new()))
        .await?
        .context("second scan not recorded")?;
    assert_eq!(second_scan.fixed, 1, "fp-a is gone from the file");

    let (summary, rules) = f.handle.fleet().code_scan_summary(since).await?;
    assert_eq!(
        (summary.scans, summary.findings_new, summary.findings_fixed),
        (2, 2, 1)
    );
    let fixed_unsafe = rules
        .iter()
        .find(|r| r.rule_id == "rust-unsafe")
        .context("no rust-unsafe count")?;
    assert_eq!(
        (
            fixed_unsafe.found,
            fixed_unsafe.delivered,
            fixed_unsafe.fixed
        ),
        (1, 1, 1)
    );
    Ok(())
}

#[tokio::test]
async fn a_skipped_scan_closes_nothing_and_a_deleted_file_supersedes() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = project(pool).await?;
    let events: Vec<_> = (0..3)
        .map(|n| {
            let mut e = event(project_id, ChangeKind::ShellChange, &["src/a.rs"]);
            e.at += Duration::seconds(n);
            e
        })
        .collect();
    let ids = f.handle.fleet().insert_change_events(&events).await?;
    f.handle
        .fleet()
        .record_code_scan(&scan_of(
            ids[0],
            &["fp-a"],
            vec![finding("rust-unsafe", "fp-a")],
        ))
        .await?;
    let mut skipped = scan_of(ids[1], &[], Vec::new());
    skipped.skipped = Some("too_large".to_owned());
    let recorded = f
        .handle
        .fleet()
        .record_code_scan(&skipped)
        .await?
        .context("skipped")?;
    assert_eq!(recorded.fixed, 0);
    let superseded = f
        .handle
        .fleet()
        .supersede_deleted_paths(CHECKOUT, &["src/a.rs".to_owned()])
        .await?;
    assert_eq!(superseded, 1);
    let rows = f
        .handle
        .fleet()
        .list_code_findings(Utc::now() - Duration::hours(1), None, 10)
        .await?;
    assert_eq!(
        rows.iter().map(|r| r.outcome.as_str()).collect::<Vec<_>>(),
        vec!["superseded"]
    );
    Ok(())
}

async fn told(handle: &crate::store::persistence::PersistHandle) -> anyhow::Result<Vec<i64>> {
    let since = Utc::now() - Duration::hours(1);
    let ids: Vec<i64> = handle
        .fleet()
        .list_code_findings(since, None, 50)
        .await?
        .iter()
        .filter(|r| r.checkout == CHECKOUT)
        .map(|r| r.id)
        .collect();
    let delivered: Vec<Delivered> = ids
        .iter()
        .map(|id| Delivered {
            finding_id: *id,
            request_id: None,
            via: mike_base::code_notes::VIA_PUSH,
        })
        .collect();
    handle
        .fleet()
        .mark_code_findings_delivered(&delivered)
        .await?;
    Ok(ids)
}

async fn outcomes(
    handle: &crate::store::persistence::PersistHandle,
) -> anyhow::Result<Vec<String>> {
    let since = Utc::now() - Duration::hours(1);
    Ok(handle
        .fleet()
        .list_code_findings(since, None, 50)
        .await?
        .into_iter()
        .filter(|r| r.checkout == CHECKOUT)
        .map(|r| r.outcome)
        .collect())
}

#[tokio::test]
async fn a_fix_by_another_session_and_a_commit_that_keeps_the_finding_are_told_apart()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = project(pool).await?;
    let author = session(pool, project_id, None).await?;
    let other = session(pool, project_id, None).await?;
    let mut base = event(project_id, ChangeKind::Edit, &["src/a.rs"]);
    base.session_id = Some(author);
    let mut fixer = base.clone();
    fixer.session_id = Some(other);
    fixer.at += Duration::seconds(1);
    let ids = f
        .handle
        .fleet()
        .insert_change_events(&[base, fixer])
        .await?;
    f.handle
        .fleet()
        .record_code_scan(&scan_of(
            ids[0],
            &["fp-a"],
            vec![finding("rust-unsafe", "fp-a")],
        ))
        .await?;
    told(&f.handle).await?;
    f.handle
        .fleet()
        .record_code_scan(&scan_of(ids[1], &[], Vec::new()))
        .await?;
    f.handle.fleet().settle_code_findings().await?;
    assert_eq!(outcomes(&f.handle).await?, vec!["fixed_by_other"]);
    let counts = f
        .handle
        .fleet()
        .session_code_counts(session_uuid(pool, author).await?)
        .await?;
    assert_eq!((counts.found, counts.told, counts.files_scanned), (1, 1, 1));
    Ok(())
}

#[tokio::test]
async fn a_told_finding_still_there_at_commit_is_ignored_until_it_is_fixed() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = project(pool).await?;
    let author = session(pool, project_id, None).await?;
    let mut edit = event(project_id, ChangeKind::Edit, &["src/a.rs"]);
    edit.session_id = Some(author);
    let ids = f
        .handle
        .fleet()
        .insert_change_events(&[edit.clone()])
        .await?;
    f.handle
        .fleet()
        .record_code_scan(&scan_of(
            ids[0],
            &["fp-c"],
            vec![finding("rust-temp-dir", "fp-c")],
        ))
        .await?;
    told(&f.handle).await?;
    let mut commit = event(project_id, ChangeKind::Commit, &["src/a.rs", "src/b.rs"]);
    commit.at = Utc::now() + Duration::seconds(5);
    let mut later = edit;
    later.at = commit.at + Duration::seconds(1);
    let more = f
        .handle
        .fleet()
        .insert_change_events(&[commit, later])
        .await?;
    let settled = f.handle.fleet().settle_code_findings().await?;
    assert!(settled.ignored >= 1);
    assert_eq!(outcomes(&f.handle).await?, vec!["ignored"]);
    f.handle
        .fleet()
        .record_code_scan(&scan_of(more[1], &[], Vec::new()))
        .await?;
    assert_eq!(outcomes(&f.handle).await?, vec!["fixed"]);
    Ok(())
}

async fn session_uuid(pool: &sqlx::PgPool, id: i64) -> anyhow::Result<uuid::Uuid> {
    Ok(
        sqlx::query_scalar("SELECT session_uuid FROM sessions.sessions WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await?,
    )
}

#[tokio::test]
async fn a_delivered_finding_gets_its_reply_read_once_after_a_minute() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = project(pool).await?;
    let ids = f
        .handle
        .fleet()
        .insert_change_events(&[event(project_id, ChangeKind::Edit, &["src/r.rs"])])
        .await?;
    f.handle
        .fleet()
        .record_code_scan(&scan_of(
            ids[0],
            &["fp-r"],
            vec![finding("rust-unsafe", "fp-r")],
        ))
        .await?;
    told(&f.handle).await?;
    sqlx::query("UPDATE workspace.code_findings SET delivered_request_id = 987654321, delivered_at = now() - interval '2 minutes' WHERE checkout = $1")
        .bind(CHECKOUT)
        .execute(pool)
        .await?;
    assert!(f.handle.fleet().record_code_finding_replies().await? >= 1);
    assert_eq!(f.handle.fleet().record_code_finding_replies().await?, 0);
    let (reply, read): (Option<String>, bool) = sqlx::query_as(
        "SELECT reply, reply_at IS NOT NULL FROM workspace.code_findings WHERE checkout = $1",
    )
    .bind(CHECKOUT)
    .fetch_one(pool)
    .await?;
    assert_eq!((reply, read), (None, true));
    Ok(())
}
