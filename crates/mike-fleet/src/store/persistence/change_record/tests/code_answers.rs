use anyhow::Context;
use serde_json::json;
use uuid::Uuid;

use super::code_scan::{finding, scan_of};
use super::fixture;
use super::seed::{CHECKOUT, event, project, session};
use crate::change_record::ChangeKind;
use crate::change_record::code_scan::answer::{Claim, answer};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use mike_base::code_notes::Delivered;

async fn found(
    handle: &PersistHandle,
    detail: serde_json::Value,
    severity: &str,
) -> anyhow::Result<(Uuid, Vec<i64>, i64)> {
    let pool = handle.pool();
    let project_id = project(pool).await?;
    let author = session(pool, project_id, None).await?;
    let uuid: Uuid = sqlx::query_scalar("SELECT session_uuid FROM sessions.sessions WHERE id = $1")
        .bind(author)
        .fetch_one(pool)
        .await?;
    let mut edit = event(project_id, ChangeKind::Edit, &["src/a.rs"]);
    edit.session_id = Some(author);
    edit.detail = detail;
    let ids = handle.fleet().insert_change_events(&[edit]).await?;
    let mut added = vec![finding("r", "fp-a"), finding("r", "fp-b")];
    for f in &mut added {
        f.severity = severity.to_owned();
    }
    handle
        .fleet()
        .record_code_scan(&scan_of(ids[0], &["fp-a", "fp-b"], added))
        .await?;
    let rows: Vec<i64> = sqlx::query_scalar("SELECT id FROM workspace.code_findings ORDER BY id")
        .fetch_all(pool)
        .await?;
    Ok((uuid, rows, project_id))
}

async fn outcomes(handle: &PersistHandle) -> anyhow::Result<Vec<String>> {
    Ok(
        sqlx::query_scalar("SELECT outcome FROM workspace.code_findings ORDER BY id")
            .fetch_all(handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn one_number_covers_the_group_and_proof_refuses_not_mine() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (uuid, ids, _) = found(
        &f.handle,
        json!({"author": "tool_call", "tool_name": "Edit"}),
        "warning",
    )
    .await?;
    let ruling = answer(&f.handle, None, ids[1], uuid, Claim::NotMine, "").await?;
    assert_eq!((ruling.answer, ruling.findings), ("refused", 2));
    assert_eq!(outcomes(&f.handle).await?, ["open", "open"]);

    let other = answer(&f.handle, None, ids[0], Uuid::new_v4(), Claim::NotMine, "").await?;
    assert_eq!(
        other.answer, "gone",
        "a session answers only its own findings"
    );
    f.handle
        .fleet()
        .record_code_answer(&ids, "kept", "why", "Accepted")
        .await?;
    assert_eq!(outcomes(&f.handle).await?, ["kept", "kept"]);
    assert_eq!(f.handle.fleet().code_keeps_today(uuid, "r").await?, 1);
    Ok(())
}

#[tokio::test]
async fn an_inferred_shell_change_can_be_disowned() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let detail = json!({"author": "shell", "inferred": true});
    let (uuid, ids, _) = found(&f.handle, detail, "warning").await?;
    let ruling = answer(&f.handle, None, ids[0], uuid, Claim::NotMine, "").await?;
    assert_eq!(ruling.answer, "not_mine");
    assert_eq!(outcomes(&f.handle).await?, ["not_mine", "not_mine"]);
    Ok(())
}

#[tokio::test]
async fn an_error_waits_for_the_operator_and_stays_open_until_accepted() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let detail = json!({"author": "tool_call"});
    let (uuid, ids, _) = found(&f.handle, detail, "error").await?;
    let reason = "The value is a constant in this file.";
    let ruling = answer(&f.handle, None, ids[0], uuid, Claim::Keep, reason).await?;
    assert_eq!(ruling.answer, "waiting");
    sqlx::query("UPDATE workspace.code_findings SET delivered_at = now() - interval '2 hours'")
        .execute(f.handle.pool())
        .await?;
    f.handle.fleet().settle_code_findings().await?;
    assert_eq!(outcomes(&f.handle).await?, ["open", "open"]);
    let listed = f.handle.fleet().code_findings_for_operator().await?;
    let row = listed.first().context("nothing waits for the operator")?;
    assert_eq!((listed.len(), row.lines.len()), (1, 2));
    let accepted = f.handle.fleet().accept_waiting_code_answer(row.id).await?;
    assert_eq!(accepted.len(), 2);
    assert_eq!(outcomes(&f.handle).await?, ["kept", "kept"]);
    Ok(())
}

#[tokio::test]
async fn a_committed_unanswered_finding_is_reminded_once() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let detail = json!({"author": "tool_call"});
    let (uuid, ids, project_id) = found(&f.handle, detail, "warning").await?;
    f.handle
        .fleet()
        .mark_code_findings_delivered(&[Delivered {
            finding_id: ids[0],
            request_id: Some(1),
            via: mike_base::code_notes::VIA_REQUEST,
        }])
        .await?;
    let mut commit = event(project_id, ChangeKind::Commit, &["src/a.rs"]);
    commit.at = chrono::Utc::now() + chrono::Duration::seconds(5);
    f.handle.fleet().insert_change_events(&[commit]).await?;
    f.handle.fleet().settle_code_findings().await?;
    let roots = vec![CHECKOUT.to_owned()];
    let due = f.handle.fleet().code_findings_to_remind(&roots).await?;
    assert_eq!(
        due.iter().map(|d| (d.id, d.session)).collect::<Vec<_>>(),
        [(ids[0], uuid)]
    );
    let again = f.handle.fleet().code_findings_to_remind(&roots).await?;
    assert!(again.is_empty());
    Ok(())
}
