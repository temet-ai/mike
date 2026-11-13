use mike::store::persistence::fleet_repo::FleetPersist as _;
use mike::store::persistence::launch_repo::LaunchPersist as _;
mod ownership;
mod support;

use support::Fixture;

#[tokio::test]
async fn late_cross_cutting_failure_carries_closed_owners_report_to_coordinator()
-> anyhow::Result<()> {
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let report = f.report("A").await?;
    f.close_owner(false).await?;
    let output = "FAIL tests/parts.rs > naming contract\nfull gate evidence after worker closure";
    f.deliver(output).await?;
    let body = f.inbox(f.coordinator).await?;
    for expected in [
        report.note_uuid.to_string(),
        f.old.to_string(),
        f.reported.clone(),
        f.checked.clone(),
        "naming contract".into(),
        "is closed".into(),
        "tag=task:A".into(),
        "does not establish who introduced".into(),
        "Do not reopen".into(),
    ] {
        assert!(body.contains(&expected), "{body}");
    }
    let notes = f.persist.fleet().fleet_note_rows(&f.run).await?;
    let retained = notes
        .iter()
        .find(|note| note.note_uuid == report.note_uuid)
        .ok_or_else(|| anyhow::anyhow!("closed owner report missing"))?;
    assert_eq!(retained.body, report.body);
    assert_eq!(retained.data, report.data);
    let ledger = mike::launch::fleet::gate::ledger(&f.run);
    let receipt = f
        .persist
        .fleet()
        .find_tree_check(&ledger, &f.checked)
        .await?
        .ok_or_else(|| anyhow::anyhow!("late gate receipt missing"))?;
    assert_eq!(receipt.output, output);
    assert!(!f.repo.path().join("unavailable.log").exists());
    assert!(f.inbox(f.successor).await?.is_empty());
    assert!(f.inbox(f.producer).await?.is_empty());
    f.still_closed().await?;
    f.db.cleanup().await
}

#[tokio::test]
async fn current_successor_receives_original_report_without_contacting_closed_session()
-> anyhow::Result<()> {
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let report = f.report("A").await?;
    let unrelated = f.report("B").await?;
    f.close_owner(true).await?;
    f.deliver("FAIL tests/parts.rs > late invariant").await?;
    let body = f.inbox(f.successor).await?;
    assert!(body.contains(&report.note_uuid.to_string()), "{body}");
    assert!(body.contains(&format!("original session {}", f.old)));
    assert!(body.contains(&f.reported));
    assert!(!body.contains(&unrelated.note_uuid.to_string()));
    assert!(f.inbox(f.coordinator).await?.is_empty());
    f.still_closed().await?;
    f.db.cleanup().await
}

#[tokio::test]
async fn unowned_cross_cutting_test_is_not_attributed_to_protocol_file_owner() -> anyhow::Result<()>
{
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    f.report("B").await?;
    f.close_owner(false).await?;
    f.deliver("FAIL tests/ink.test.ts > every-field-is-drawn")
        .await?;
    let body = f.inbox(f.coordinator).await?;
    assert!(body.contains("every-field-is-drawn"));
    assert!(body.contains("unowned file"));
    assert!(!body.contains("Retained task B"));
    assert!(f.inbox(f.producer).await?.is_empty());
    assert!(f.inbox(f.successor).await?.is_empty());
    f.still_closed().await?;
    f.db.cleanup().await
}

#[tokio::test]
async fn absent_task_report_remains_explicitly_unknown_in_late_handoff() -> anyhow::Result<()> {
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    f.report("B").await?;
    f.close_owner(false).await?;
    f.deliver("FAIL tests/parts.rs > missing report").await?;
    let body = f.inbox(f.coordinator).await?;
    assert!(body.contains("No retained report was found for this task in this run"));
    assert!(body.contains("establish missing evidence"));
    assert!(!body.contains("Report note "));
    f.still_closed().await?;
    f.db.cleanup().await
}

#[tokio::test]
async fn long_report_history_keeps_three_latest_references_and_exact_omission_count()
-> anyhow::Result<()> {
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let mut reports = Vec::new();
    for _ in 0..5 {
        reports.push(f.report("A").await?);
    }
    f.close_owner(false).await?;
    f.deliver("FAIL tests/parts.rs > retained history").await?;
    let body = f.inbox(f.coordinator).await?;
    assert_eq!(body.matches("Report note ").count(), 3, "{body}");
    assert!(body.contains("2 older reports omitted"));
    assert!(body.contains("full task history, including closed sessions"));
    for report in &reports[2..] {
        assert!(body.contains(&report.note_uuid.to_string()));
    }
    for report in &reports[..2] {
        assert!(!body.contains(&report.note_uuid.to_string()));
    }
    assert!(body.len() < 10 * 1024);
    assert_eq!(f.persist.fleet().fleet_note_rows(&f.run).await?.len(), 5);
    f.still_closed().await?;
    f.db.cleanup().await
}

#[tokio::test]
async fn report_with_unusable_commit_preserves_lookup_without_repeating_unbounded_payload()
-> anyhow::Result<()> {
    use mike::store::persistence::session_notes::NewSessionNote;
    use serde_json::json;

    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let tags = vec![
        format!("fleet:{}", f.row.fleet_uuid),
        "report".into(),
        "task:A".into(),
    ];
    let oversized = "unverified payload".repeat(10_000);
    let note = f
        .persist
        .launch()
        .create_session_note(
            f.old,
            NewSessionNote {
                kind: Some("report"),
                title: "Incomplete report",
                body: Some(&oversized),
                status: None,
                tags: Some(&tags),
                data: Some(&json!({"task": "A", "commit": oversized})),
            },
        )
        .await?;
    f.close_owner(false).await?;
    f.deliver("FAIL tests/parts.rs > unknown reported commit")
        .await?;
    let body = f.inbox(f.coordinator).await?;
    assert!(body.contains(&note.note_uuid.to_string()));
    assert!(body.contains("reported commit unknown"));
    assert!(!body.contains("unverified payload"));
    assert!(body.len() < 10 * 1024);
    assert_eq!(
        f.persist.fleet().fleet_note_rows(&f.run).await?[0].body,
        oversized
    );
    f.still_closed().await?;
    f.db.cleanup().await
}
