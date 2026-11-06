use serde::Deserialize;
use serde_json::value::RawValue;

use super::RemindersOutcome;
use super::seat_role::SeatRole;

#[derive(Deserialize)]
struct Messages<'a> {
    #[serde(borrow, default)]
    messages: Vec<&'a RawValue>,
}

pub(super) fn earlier_messages_intact(before: &[u8], after: &[u8]) -> bool {
    let (Ok(before), Ok(after)) = (
        serde_json::from_slice::<Messages<'_>>(before),
        serde_json::from_slice::<Messages<'_>>(after),
    ) else {
        return false;
    };
    let Some(earlier) = before.messages.len().checked_sub(1) else {
        return false;
    };
    before.messages.len() == after.messages.len()
        && before.messages[..earlier]
            .iter()
            .zip(&after.messages)
            .all(|(was, is)| was.get() == is.get())
}

pub(super) fn keep_inherited_prefix(
    role: SeatRole,
    before: &[u8],
    outcome: RemindersOutcome,
) -> RemindersOutcome {
    match outcome {
        RemindersOutcome::Appended { ref body, .. }
            if role == SeatRole::Fork && !earlier_messages_intact(before, body) =>
        {
            RemindersOutcome::Unchanged
        }
        other => other,
    }
}
