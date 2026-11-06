use super::SessionNoteRow;
use crate::launch::fleet::{FleetDefinition, acceptance_register, status::NoteFacts};
use crate::store::persistence::PersistError;

pub(super) async fn validate(
    connection: &mut sqlx::PgConnection,
    row: &SessionNoteRow,
) -> Result<(), PersistError> {
    if !row.tags.iter().any(|tag| tag == "acceptance-obligation") {
        return Ok(());
    }
    let fleets: Vec<_> = row
        .tags
        .iter()
        .filter_map(|tag| tag.strip_prefix("fleet:"))
        .collect();
    if fleets.len() != 1 {
        return Err(invalid("exactly one fleet UUID tag is required"));
    }
    let fleet: uuid::Uuid = fleets[0]
        .parse()
        .map_err(|_| invalid("invalid fleet UUID tag"))?;
    let definition = sqlx::query_scalar!(
        "SELECT definition FROM launch.fleets WHERE fleet_uuid = $1 FOR SHARE",
        fleet,
    )
    .fetch_optional(&mut *connection)
    .await?;
    let definition = definition.ok_or_else(|| invalid("unknown fleet"))?;
    let definition = FleetDefinition::from_value(&definition)
        .map_err(|error| invalid(format!("invalid fleet definition: {error:?}")))?;
    let author = sqlx::query_scalar!(
        r#"SELECT s.session_uuid AS "session_uuid!"
             FROM workspace.session_notes n
             JOIN sessions.sessions s ON s.id = n.session_id
            WHERE n.note_uuid = $1"#,
        row.note_uuid,
    )
    .fetch_one(&mut *connection)
    .await?;
    let note = NoteFacts {
        note_uuid: row.note_uuid,
        session_uuid: author,
        role: None,
        seat: None,
        kind: row.kind.clone(),
        title: row.title.clone(),
        body: row.body.clone(),
        status: row.status.clone(),
        tags: row.tags.clone(),
        data: row.data.clone(),
        created_at: row.created_at,
        updated_at: row.updated_at,
    };
    acceptance_register::validate_write(fleets[0], &definition, &note).map_err(invalid)
}

fn invalid(message: impl Into<String>) -> PersistError {
    PersistError::InvalidAcceptanceObligation(message.into())
}

pub(super) async fn protect_update(
    connection: &mut sqlx::PgConnection,
    note: uuid::Uuid,
    update: super::SessionNoteUpdate<'_>,
) -> Result<(), PersistError> {
    let previous = locked(connection, note).await?;
    let Some(previous) = previous else {
        return Ok(());
    };
    if !is_obligation(&previous) {
        if update
            .tags
            .is_some_and(|tags| tags.iter().any(|tag| tag == "acceptance-obligation"))
        {
            return Err(invalid(
                "an existing note cannot become an acceptance obligation; create a new acceptance-obligation note so its creation time records the new requirement",
            ));
        }
        return Ok(());
    }
    if is_ruling(&previous) {
        return immutable(&previous, update).map_err(invalid);
    }
    super::obligation_identity::protect(connection, &previous, update).await
}

pub(super) async fn protect_delete(
    connection: &mut sqlx::PgConnection,
    note: uuid::Uuid,
) -> Result<(), PersistError> {
    if locked(connection, note)
        .await?
        .as_ref()
        .is_some_and(is_ruling)
    {
        return Err(invalid(
            "executable ruling records are immutable; supersede the ruling with a new acceptance note",
        ));
    }
    Ok(())
}

async fn locked(
    connection: &mut sqlx::PgConnection,
    note: uuid::Uuid,
) -> Result<Option<SessionNoteRow>, PersistError> {
    Ok(sqlx::query_as!(
        SessionNoteRow,
        "SELECT note_uuid, kind, title, body, status, tags, data, created_at, updated_at
           FROM workspace.session_notes
          WHERE note_uuid = $1
            FOR UPDATE",
        note,
    )
    .fetch_optional(connection)
    .await?)
}

fn is_obligation(row: &SessionNoteRow) -> bool {
    row.tags.iter().any(|tag| tag == "acceptance-obligation")
}

fn is_ruling(row: &SessionNoteRow) -> bool {
    is_obligation(row) && row.data.get("ruling").is_some()
}

fn immutable(
    previous: &SessionNoteRow,
    update: super::SessionNoteUpdate<'_>,
) -> Result<(), String> {
    let data = update.data.unwrap_or(&previous.data);
    let same_scope = [
        "task",
        "tasks",
        "package",
        "packages",
        "criteria",
        "obligation_id",
        "supersedes",
        "ruling",
    ]
    .iter()
    .all(|key| data.get(key) == previous.data.get(key));
    let same_required = super::obligation_identity::required_unchanged(&previous.data, data);
    if !same_scope
        || !same_required
        || update.kind.is_some_and(|kind| kind != previous.kind)
        || update.tags.is_some_and(|tags| tags != previous.tags)
    {
        return Err("ruling command, version, expected result, scope, required flag and tags are immutable; create a new version with supersedes naming this note UUID".into());
    }
    Ok(())
}

#[cfg(test)]
mod ruling_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn immutable_ruling_accepts_observation_but_rejects_command_scope_or_tag_removal() {
        let previous = SessionNoteRow {
            note_uuid: uuid::Uuid::nil(),
            kind: "task".into(),
            title: "Q29".into(),
            body: String::new(),
            status: None,
            tags: vec!["acceptance-obligation".into()],
            data: json!({"tasks": ["T1"], "ruling": {"version": 1, "command": "true", "expected_exit_code": 0}}),
            created_at: chrono::DateTime::default(),
            updated_at: chrono::DateTime::default(),
        };
        let mut accepted = previous.data.clone();
        accepted["outcome"] = json!("accepted");
        assert!(
            immutable(
                &previous,
                super::super::SessionNoteUpdate {
                    data: Some(&accepted),
                    ..Default::default()
                }
            )
            .is_ok()
        );
        for key in ["ruling", "tasks", "obligation_id", "supersedes"] {
            let mut changed = previous.data.clone();
            changed[key] = json!(null);
            assert!(
                immutable(
                    &previous,
                    super::super::SessionNoteUpdate {
                        data: Some(&changed),
                        ..Default::default()
                    }
                )
                .is_err()
            );
        }
        assert!(
            immutable(
                &previous,
                super::super::SessionNoteUpdate {
                    tags: Some(&[]),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
