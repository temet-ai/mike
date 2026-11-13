use mike::store::persistence::fleet_repo::FleetPersist as _;
use mike::store::persistence::launch_repo::LaunchPersist as _;
mod promotion;
mod support;

use mike::store::persistence::PersistError;
use mike::store::persistence::session_notes::SessionNoteUpdate;
use serde_json::json;

use support::{Fixture, assert_status, obligation};

#[tokio::test]
async fn existing_obligation_accepted_after_final_refreshes_without_rewriting_verdict()
-> anyhow::Result<()> {
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let pending = f.obligation(&obligation("pending")).await?;
    let verdict = f.verdict().await?;
    assert_status(&f.status().await?, "working", false);
    let accepted = f.outcome(&pending, "accepted").await?;
    assert!(accepted.updated_at > verdict.updated_at);
    assert_eq!(accepted.note_uuid, pending.note_uuid);
    assert_eq!(accepted.created_at, pending.created_at);
    assert_eq!(accepted.title, pending.title);
    assert_eq!(accepted.body, pending.body);
    assert_eq!(accepted.tags, pending.tags);
    assert_eq!(accepted.status, pending.status);
    assert!(
        f.persist
            .launch()
            .session_note_belongs_to(f.acceptance, accepted.note_uuid)
            .await?
    );
    assert!(
        !f.persist
            .launch()
            .session_note_belongs_to(f.worker, accepted.note_uuid)
            .await?
    );
    assert_status(&f.status().await?, "passed", true);
    f.unchanged_verdict(&verdict).await?;
    let body = "Additional detail keeps the complete previous observation available";
    f.persist
        .fleet()
        .update_session_note(
            accepted.note_uuid,
            SessionNoteUpdate {
                body: Some(body),
                ..Default::default()
            },
        )
        .await?;
    assert_status(&f.status().await?, "passed", true);
    f.unchanged_verdict(&verdict).await?;
    f.cleanup().await
}

#[tokio::test]
async fn reopened_obligation_blocks_until_current_outcome_is_accepted_again() -> anyhow::Result<()>
{
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let mut note = f.obligation(&obligation("accepted")).await?;
    let verdict = f.verdict().await?;
    assert_status(&f.status().await?, "passed", true);
    for outcome in [
        "pending",
        "blocked",
        "failed",
        "not_exercised",
        "invalidated",
    ] {
        note = f.outcome(&note, outcome).await?;
        let state = if outcome == "failed" {
            "rework"
        } else {
            "working"
        };
        assert_status(&f.status().await?, state, false);
        note = f.outcome(&note, "accepted").await?;
        assert_status(&f.status().await?, "passed", true);
    }
    f.unchanged_verdict(&verdict).await?;
    f.cleanup().await
}

#[tokio::test]
async fn new_and_superseding_obligations_require_final_verdict_covering_their_creation()
-> anyhow::Result<()> {
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let original = f.obligation(&obligation("accepted")).await?;
    let original_verdict = f.verdict().await?;
    assert_status(&f.status().await?, "passed", true);
    let mut added = obligation("accepted");
    added["obligation_id"] = json!("new-regression");
    let added = f.obligation(&added).await?;
    assert!(added.created_at > original_verdict.updated_at);
    assert_status(&f.status().await?, "working", false);
    let fresh = f.verdict().await?;
    assert_status(&f.status().await?, "passed", true);
    let mut successor = obligation("pending");
    successor["supersedes"] = json!(original.note_uuid);
    let successor = f.obligation(&successor).await?;
    assert!(successor.created_at > fresh.updated_at);
    assert_status(&f.status().await?, "working", false);
    let successor = f.outcome(&successor, "accepted").await?;
    assert_status(&f.status().await?, "working", false);
    f.verdict().await?;
    let status = f.status().await?;
    assert_status(&status, "passed", true);
    assert_eq!(status.obligations.len(), 2);
    assert!(
        status
            .obligations
            .iter()
            .any(|entry| entry.note_id == successor.note_uuid)
    );
    let notes = f.persist.fleet().fleet_note_rows(&f.run).await?;
    let retained = notes
        .iter()
        .find(|note| note.note_uuid == original.note_uuid)
        .ok_or_else(|| anyhow::anyhow!("original obligation disappeared"))?;
    assert_eq!(retained.data, original.data);
    assert_eq!(retained.body, original.body);
    assert_eq!(retained.session_uuid, f.acceptance);
    f.cleanup().await
}

#[tokio::test]
async fn valid_obligation_rejects_identity_scope_and_marker_changes_without_losing_acceptance()
-> anyhow::Result<()> {
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let note = f.obligation(&obligation("accepted")).await?;
    let verdict = f.verdict().await?;
    for patch in [
        json!({"task": "T2", "criteria": ["C2"]}),
        json!({"obligation_id": "renamed"}),
        json!({"required": false}),
        json!({"supersedes": verdict.note_uuid}),
    ] {
        let mut changed = note.data.clone();
        mike::launch::fleet::merge(&mut changed, &patch);
        let error = f
            .persist
            .fleet()
            .update_session_note(
                note.note_uuid,
                SessionNoteUpdate {
                    data: Some(&changed),
                    ..Default::default()
                },
            )
            .await;
        assert!(
            matches!(error, Err(PersistError::InvalidAcceptanceObligation(ref message)) if message.contains("supersedes"))
        );
        assert_status(&f.status().await?, "passed", true);
    }
    let tags = [format!("fleet:{}", f.row.fleet_uuid)];
    let removed = f
        .persist
        .fleet()
        .update_session_note(
            note.note_uuid,
            SessionNoteUpdate {
                tags: Some(&tags),
                ..Default::default()
            },
        )
        .await;
    assert!(matches!(
        removed,
        Err(PersistError::InvalidAcceptanceObligation(_))
    ));
    let rows = f.persist.fleet().fleet_note_rows(&f.run).await?;
    let unchanged = rows
        .iter()
        .find(|row| row.note_uuid == note.note_uuid)
        .ok_or_else(|| anyhow::anyhow!("obligation disappeared"))?;
    assert_eq!(unchanged.data, note.data);
    assert_eq!(unchanged.tags, note.tags);
    assert_eq!(unchanged.updated_at, note.updated_at);
    assert_status(&f.status().await?, "passed", true);
    f.unchanged_verdict(&verdict).await?;
    f.cleanup().await
}

#[tokio::test]
async fn equivalent_scope_aliases_and_order_keep_existing_obligation_freshness()
-> anyhow::Result<()> {
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let mut data = obligation("pending");
    data.as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("obligation must be an object"))?
        .remove("task");
    data["tasks"] = json!(["T2", "T1"]);
    data["criteria"] = json!(["C2", "C1"]);
    let note = f.obligation(&data).await?;
    let verdict = f.verdict().await?;
    let data = json!({
        "obligation_id": "setting-journey", "packages": ["T1", "T2", "T1"],
        "criteria": ["C1", "C2"], "outcome": "accepted", "observation": "Both outcomes observed"
    });
    let mut tags = f.tags();
    tags.reverse();
    tags.push("extra-observation-tag".into());
    let updated = f
        .persist
        .fleet()
        .update_session_note(
            note.note_uuid,
            SessionNoteUpdate {
                data: Some(&data),
                tags: Some(&tags),
                ..Default::default()
            },
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("obligation disappeared"))?;
    assert_eq!(updated.data, data);
    assert_eq!(updated.tags, tags);
    assert_eq!(updated.body, note.body);
    assert_status(&f.status().await?, "passed", true);
    f.unchanged_verdict(&verdict).await?;
    f.cleanup().await
}

#[tokio::test]
async fn malformed_legacy_obligation_can_be_retired_without_rewriting_its_content()
-> anyhow::Result<()> {
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let original = f.obligation(&obligation("accepted")).await?;
    let verdict = f.verdict().await?;
    let mut malformed = original.data.clone();
    malformed["task"] = json!("unknown");
    sqlx::query("UPDATE workspace.session_notes SET data = $2 WHERE note_uuid = $1")
        .bind(original.note_uuid)
        .bind(&malformed)
        .execute(f.persist.pool())
        .await?;
    let invalid = f.status().await?;
    assert_status(&invalid, "passed", false);
    assert!(invalid.obligations[0].issue.is_some());
    let tags = [format!("fleet:{}", f.row.fleet_uuid)];
    let retired = f
        .persist
        .fleet()
        .update_session_note(
            original.note_uuid,
            SessionNoteUpdate {
                tags: Some(&tags),
                ..Default::default()
            },
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("legacy note disappeared"))?;
    assert_eq!(retired.data, malformed);
    assert_eq!(retired.body, original.body);
    assert_eq!(retired.title, original.title);
    assert_eq!(retired.created_at, original.created_at);
    assert!(
        f.persist
            .launch()
            .session_note_belongs_to(f.acceptance, retired.note_uuid)
            .await?
    );
    f.obligation(&obligation("accepted")).await?;
    assert_status(&f.status().await?, "working", false);
    f.unchanged_verdict(&verdict).await?;
    f.verdict().await?;
    assert_status(&f.status().await?, "passed", true);
    f.cleanup().await
}

#[tokio::test]
async fn malformed_legacy_observation_can_be_repaired_without_changing_identity()
-> anyhow::Result<()> {
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let original = f.obligation(&obligation("accepted")).await?;
    let verdict = f.verdict().await?;
    let mut malformed = original.data.clone();
    malformed["observation"] = json!(null);
    sqlx::query("UPDATE workspace.session_notes SET data = $2 WHERE note_uuid = $1")
        .bind(original.note_uuid)
        .bind(&malformed)
        .execute(f.persist.pool())
        .await?;
    let invalid = f.status().await?;
    assert_status(&invalid, "working", false);
    assert!(invalid.obligations[0].issue.is_some());
    let repaired = f
        .persist
        .fleet()
        .update_session_note(
            original.note_uuid,
            SessionNoteUpdate {
                data: Some(&original.data),
                ..Default::default()
            },
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("legacy obligation disappeared"))?;
    assert_eq!(repaired.data, original.data);
    assert_eq!(repaired.created_at, original.created_at);
    assert_eq!(repaired.body, original.body);
    assert_status(&f.status().await?, "passed", true);
    f.unchanged_verdict(&verdict).await?;
    f.cleanup().await
}
