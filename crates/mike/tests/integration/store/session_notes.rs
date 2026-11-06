use mike::store::persistence::PersistHandle;
use mike::store::persistence::session_goals::SetSessionIdentity;
use mike::store::persistence::session_notes::{
    NewSessionNote, NoteOrder, ProjectNoteFilter, SessionNoteFilter, SessionNoteRow,
    SessionNoteUpdate,
};
use uuid::Uuid;

use super::workspace::support::{register_input, shared_handle, unique};

fn note(title: &str) -> NewSessionNote<'_> {
    NewSessionNote {
        kind: None,
        title,
        body: None,
        status: None,
        tags: None,
        data: None,
    }
}

fn session_filter<'a>(order: NoteOrder, q: Option<&'a str>) -> SessionNoteFilter<'a> {
    SessionNoteFilter {
        kind: None,
        status: None,
        tag: None,
        q,
        order,
        limit: 50,
        offset: 0,
    }
}

fn project_filter(role: Option<&str>) -> ProjectNoteFilter<'_> {
    ProjectNoteFilter {
        role,
        name: None,
        kind: None,
        status: None,
        tag: None,
        session_uuid: None,
        q: None,
        order: NoteOrder::NewestFirst,
        limit: 50,
        offset: 0,
    }
}

fn uuids(notes: &[SessionNoteRow]) -> Vec<Uuid> {
    notes.iter().map(|n| n.note_uuid).collect()
}

async fn registered(handle: &PersistHandle, project_path: &str) -> anyhow::Result<(Uuid, i64)> {
    let session = Uuid::new_v4();
    let row = handle
        .register_workspace_session(register_input(session, project_path))
        .await?;
    Ok((session, row.project_id))
}

#[tokio::test]
async fn session_note_lifecycle_defaults_partial_update_and_delete() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let (session, _) = registered(&handle, &unique("notes-lifecycle")).await?;

    let created = handle
        .create_session_note(session, note("first finding"))
        .await?;
    assert_eq!(created.kind, "note");
    assert_eq!(created.body, "");
    assert!(created.status.is_none() && created.tags.is_empty());
    assert_eq!(created.data, serde_json::json!({}));

    let tags = ["schema".to_string()];
    let task = handle
        .create_session_note(
            session,
            NewSessionNote {
                kind: Some("task"),
                body: Some("write the DDL"),
                status: Some("open"),
                tags: Some(&tags),
                ..note("ship the migration")
            },
        )
        .await?;
    let done = handle
        .update_session_note(
            task.note_uuid,
            SessionNoteUpdate {
                status: Some("done"),
                ..Default::default()
            },
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("note missing after update"))?;
    assert_eq!(done.status.as_deref(), Some("done"));
    assert_eq!(done.title, "ship the migration");
    assert_eq!(done.body, "write the DDL");
    assert_eq!(done.tags, tags.to_vec());

    let data = serde_json::json!({"loc": "migrations/"});
    let replaced = handle
        .update_session_note(
            task.note_uuid,
            SessionNoteUpdate {
                data: Some(&data),
                ..Default::default()
            },
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("note missing after data update"))?;
    assert_eq!(replaced.data, data);
    assert_eq!(replaced.status.as_deref(), Some("done"));

    assert!(handle.delete_session_note(created.note_uuid).await?);
    assert!(!handle.delete_session_note(created.note_uuid).await?);
    let remaining = handle
        .fetch_session_notes(session, session_filter(NoteOrder::NewestFirst, None))
        .await?;
    assert_eq!(uuids(&remaining), vec![task.note_uuid]);
    Ok(())
}

#[tokio::test]
async fn session_notes_order_and_search() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let (session, _) = registered(&handle, &unique("notes-order")).await?;
    let first = handle
        .create_session_note(session, note("gate blocks on stale seeds"))
        .await?;
    let second = handle
        .create_session_note(session, note("unrelated finding"))
        .await?;

    let newest = handle
        .fetch_session_notes(session, session_filter(NoteOrder::NewestFirst, None))
        .await?;
    assert_eq!(uuids(&newest), vec![second.note_uuid, first.note_uuid]);
    let oldest = handle
        .fetch_session_notes(session, session_filter(NoteOrder::OldestFirst, None))
        .await?;
    assert_eq!(uuids(&oldest), vec![first.note_uuid, second.note_uuid]);
    let searched = handle
        .fetch_session_notes(
            session,
            session_filter(NoteOrder::NewestFirst, Some("stale seeds")),
        )
        .await?;
    assert_eq!(uuids(&searched), vec![first.note_uuid]);
    Ok(())
}

#[tokio::test]
async fn project_notes_filter_by_role_across_sessions() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let project_path = unique("notes-project");
    let (observer, project_id) = registered(&handle, &project_path).await?;
    let (developer, _) = registered(&handle, &project_path).await?;
    let seats = [
        (
            observer,
            "observer-1",
            "observer",
            "the gate blocks on stale seeds",
        ),
        (
            developer,
            "rust-dev-alpha",
            "rust-developer",
            "schema landed",
        ),
    ];
    for (session, name, role, finding) in seats {
        handle
            .set_session_goal(SetSessionIdentity {
                session_uuid: session,
                display_name: Some(name),
                role: Some(role),
                goal: None,
                next_steps: None,
                instructions: None,
                set_by: "mcp",
            })
            .await?;
        handle
            .create_session_note(
                session,
                NewSessionNote {
                    kind: Some("finding"),
                    ..note(finding)
                },
            )
            .await?;
    }

    let (total, rows) = handle
        .fetch_project_notes(project_id, project_filter(Some("OBSERVER")))
        .await?;
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].session_uuid, observer);
    assert_eq!(rows[0].display_name.as_deref(), Some("observer-1"));
    assert_eq!(rows[0].role.as_deref(), Some("observer"));
    assert_eq!(rows[0].title, "the gate blocks on stale seeds");

    let (all_total, all_rows) = handle
        .fetch_project_notes(project_id, project_filter(None))
        .await?;
    assert_eq!(all_total, 2);
    assert_eq!(all_rows.len(), 2);
    Ok(())
}
