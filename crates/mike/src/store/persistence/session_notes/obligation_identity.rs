use super::{SessionNoteRow, SessionNoteUpdate};
use crate::launch::fleet::acceptance_register::equivalent_scope;
use crate::store::persistence::PersistError;

pub(super) async fn protect(
    connection: &mut sqlx::PgConnection,
    previous: &SessionNoteRow,
    update: SessionNoteUpdate<'_>,
) -> Result<(), PersistError> {
    if unchanged(previous, update) {
        return Ok(());
    }
    if update
        .tags
        .is_some_and(|tags| !tags.iter().any(|tag| tag == "acceptance-obligation"))
    {
        match super::obligation::validate(connection, previous).await {
            Err(PersistError::InvalidAcceptanceObligation(_)) => return Ok(()),
            Err(error) => return Err(error),
            Ok(()) => {}
        }
    }
    Err(PersistError::InvalidAcceptanceObligation(
        "acceptance obligation identity, scope, required flag and fleet are immutable; create a new note with supersedes naming this note UUID; malformed legacy notes may have their acceptance-obligation tag removed before creating a corrected obligation".into(),
    ))
}

fn unchanged(previous: &SessionNoteRow, update: SessionNoteUpdate<'_>) -> bool {
    let data = update.data.unwrap_or(&previous.data);
    let protected = ["obligation_id", "supersedes", "ruling"]
        .iter()
        .all(|key| data.get(key) == previous.data.get(key));
    let scope = data == &previous.data || equivalent_scope(&previous.data, data);
    protected
        && scope
        && required_unchanged(&previous.data, data)
        && update.kind.is_none_or(|kind| kind == previous.kind)
        && update
            .tags
            .is_none_or(|tags| identity_tags(tags) == identity_tags(&previous.tags))
}

fn identity_tags(tags: &[String]) -> Vec<&str> {
    let mut tags: Vec<_> = tags
        .iter()
        .map(String::as_str)
        .filter(|tag| *tag == "acceptance-obligation" || tag.starts_with("fleet:"))
        .collect();
    tags.sort_unstable();
    tags.dedup();
    tags
}

pub(super) fn required_unchanged(before: &serde_json::Value, after: &serde_json::Value) -> bool {
    let required = |data: &serde_json::Value| {
        data.get("required")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true)
    };
    required(before) == required(after)
}
