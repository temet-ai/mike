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
            request_id: 4242,
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
    let mut deleted = scan_of(ids[2], &[], Vec::new());
    deleted.deleted = true;
    deleted.skipped = Some("deleted".to_owned());
    f.handle.fleet().record_code_scan(&deleted).await?;
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
