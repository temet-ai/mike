use mike::store::persistence::PersistError;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use mike::store::persistence::session_notes::{
    NewSessionNote, NoteOrder, SessionNoteFilter, SessionNoteRow, SessionNoteUpdate,
};
use serde_json::json;
use uuid::Uuid;

use super::support::{Fixture, assert_status, obligation};

async fn ordinary(f: &Fixture, kind: &str) -> anyhow::Result<SessionNoteRow> {
    let tags = [format!("fleet:{}", f.row.fleet_uuid)];
    Ok(f.persist
        .launch()
        .create_session_note(
            f.acceptance,
            NewSessionNote {
                kind: Some(kind),
                title: "Older ordinary note",
                body: Some("The note retains its original content and identity"),
                status: Some("open"),
                tags: Some(&tags),
                data: Some(&obligation("accepted")),
            },
        )
        .await?)
}

async fn stored(f: &Fixture, id: Uuid) -> anyhow::Result<SessionNoteRow> {
    f.persist
        .launch()
        .fetch_session_notes(
            f.acceptance,
            SessionNoteFilter {
                kind: None,
                status: None,
                tag: None,
                q: None,
                order: NoteOrder::OldestFirst,
                limit: 100,
                offset: 0,
            },
        )
        .await?
        .into_iter()
        .find(|note| note.note_uuid == id)
        .ok_or_else(|| anyhow::anyhow!("note disappeared"))
}

#[tokio::test]
async fn old_ordinary_notes_cannot_be_promoted_by_tag_or_combined_kind_changes()
-> anyhow::Result<()> {
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let notes = [ordinary(&f, "task").await?, ordinary(&f, "note").await?];
    let verdict = f.verdict().await?;
    for note in notes {
        let edited = f
            .persist
            .launch()
            .update_session_note(
                note.note_uuid,
                SessionNoteUpdate {
                    body: Some("A normal body edit remains supported"),
                    ..Default::default()
                },
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("ordinary note disappeared"))?;
        assert_eq!(edited.created_at, note.created_at);
        for kind in [None, Some("task")] {
            let result = f
                .persist
                .launch()
                .update_session_note(
                    note.note_uuid,
                    SessionNoteUpdate {
                        kind,
                        tags: Some(&f.tags()),
                        data: Some(&obligation("accepted")),
                        ..Default::default()
                    },
                )
                .await;
            assert!(
                matches!(result, Err(PersistError::InvalidAcceptanceObligation(ref message)) if message.contains("create a new"))
            );
            let retained = stored(&f, note.note_uuid).await?;
            assert_eq!(retained.tags, edited.tags);
            assert_eq!(retained.data, edited.data);
            assert_eq!(retained.body, edited.body);
            assert_eq!(retained.updated_at, edited.updated_at);
        }
        let converted = f
            .persist
            .launch()
            .update_session_note(
                note.note_uuid,
                SessionNoteUpdate {
                    kind: Some("task"),
                    ..Default::default()
                },
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("ordinary note disappeared"))?;
        assert_eq!(converted.kind, "task");
        assert_eq!(converted.tags, edited.tags);
    }
    assert_status(&f.status().await?, "passed", true);
    f.unchanged_verdict(&verdict).await?;
    f.obligation(&obligation("accepted")).await?;
    assert_status(&f.status().await?, "working", false);
    f.cleanup().await
}

#[tokio::test]
async fn malformed_kind_cannot_promote_in_place_or_reenter_after_quarantine() -> anyhow::Result<()>
{
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let original = f.obligation(&obligation("accepted")).await?;
    sqlx::query("UPDATE workspace.session_notes SET kind = 'note' WHERE note_uuid = $1")
        .bind(original.note_uuid)
        .execute(f.persist.pool())
        .await?;
    let verdict = f.verdict().await?;
    let promoted = f
        .persist
        .launch()
        .update_session_note(
            original.note_uuid,
            SessionNoteUpdate {
                kind: Some("task"),
                ..Default::default()
            },
        )
        .await;
    assert!(matches!(
        promoted,
        Err(PersistError::InvalidAcceptanceObligation(_))
    ));
    let tags = [format!("fleet:{}", f.row.fleet_uuid)];
    let retired = f
        .persist
        .launch()
        .update_session_note(
            original.note_uuid,
            SessionNoteUpdate {
                tags: Some(&tags),
                ..Default::default()
            },
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("legacy note disappeared"))?;
    let restored = f
        .persist
        .launch()
        .update_session_note(
            original.note_uuid,
            SessionNoteUpdate {
                tags: Some(&f.tags()),
                kind: Some("task"),
                ..Default::default()
            },
        )
        .await;
    assert!(matches!(
        restored,
        Err(PersistError::InvalidAcceptanceObligation(_))
    ));
    let retained = stored(&f, original.note_uuid).await?;
    assert_eq!(retained.kind, "note");
    assert_eq!(retained.tags, retired.tags);
    assert_eq!(retained.data, original.data);
    assert_eq!(retained.body, original.body);
    assert_eq!(retained.created_at, original.created_at);
    assert!(
        f.persist
            .launch()
            .session_note_belongs_to(f.acceptance, retained.note_uuid)
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
async fn optional_obligation_becomes_required_only_through_a_new_successor() -> anyhow::Result<()> {
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let mut optional = obligation("accepted");
    optional["required"] = json!(false);
    let original = f.obligation(&optional).await?;
    let verdict = f.verdict().await?;
    let mut required = optional.clone();
    required["required"] = json!(true);
    let mut implicit = optional.clone();
    implicit
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("obligation object missing"))?
        .remove("required");
    for data in [&required, &implicit] {
        let result = f
            .persist
            .launch()
            .update_session_note(
                original.note_uuid,
                SessionNoteUpdate {
                    data: Some(data),
                    ..Default::default()
                },
            )
            .await;
        assert!(matches!(
            result,
            Err(PersistError::InvalidAcceptanceObligation(_))
        ));
    }
    let retained = stored(&f, original.note_uuid).await?;
    assert_eq!(retained.data, optional);
    assert_eq!(retained.updated_at, original.updated_at);
    assert_status(&f.status().await?, "passed", true);
    required["supersedes"] = json!(original.note_uuid);
    let successor = f.obligation(&required).await?;
    assert!(successor.created_at > verdict.updated_at);
    assert_status(&f.status().await?, "working", false);
    f.unchanged_verdict(&verdict).await?;
    f.verdict().await?;
    assert_status(&f.status().await?, "passed", true);
    f.cleanup().await
}

#[tokio::test]
async fn implicit_and_explicit_required_true_are_equivalent_but_ruling_strengthening_is_not()
-> anyhow::Result<()> {
    let Some(f) = Fixture::new().await? else {
        return Ok(());
    };
    let mut data = obligation("accepted");
    let note = f.obligation(&data).await?;
    let verdict = f.verdict().await?;
    data["required"] = json!(true);
    let updated = f
        .persist
        .launch()
        .update_session_note(
            note.note_uuid,
            SessionNoteUpdate {
                data: Some(&data),
                ..Default::default()
            },
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("obligation disappeared"))?;
    assert_eq!(updated.data, data);
    assert_status(&f.status().await?, "passed", true);
    f.unchanged_verdict(&verdict).await?;
    data["obligation_id"] = json!("optional-ruling");
    data["required"] = json!(false);
    data["ruling"] = json!({"version": 1, "command": "true", "expected_exit_code": 0});
    let rule = f.obligation(&data).await?;
    data["required"] = json!(true);
    let strengthened = f
        .persist
        .launch()
        .update_session_note(
            rule.note_uuid,
            SessionNoteUpdate {
                data: Some(&data),
                ..Default::default()
            },
        )
        .await;
    assert!(matches!(
        strengthened,
        Err(PersistError::InvalidAcceptanceObligation(_))
    ));
    assert_eq!(stored(&f, rule.note_uuid).await?.data, rule.data);
    f.cleanup().await
}
