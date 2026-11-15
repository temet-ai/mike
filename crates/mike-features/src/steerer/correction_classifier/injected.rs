use crate::wire::mail_text::MAIL_LEAD;

const CHANNEL_SOURCE: &str = "<channel source=";
const PEER_LEAD: &str = "Another Claude session sent a message";
const REMINDER_OPEN: &str = "<system-reminder";
const REMINDER_CLOSE: &str = "</system-reminder";

pub fn is_injected(text: &str) -> bool {
    let trimmed = text.trim_start();
    [MAIL_LEAD, CHANNEL_SOURCE, PEER_LEAD]
        .iter()
        .any(|lead| trimmed.starts_with(lead))
        || wholly_reminders(trimmed)
}

fn wholly_reminders(text: &str) -> bool {
    let mut rest = text.trim();
    if !rest.starts_with(REMINDER_OPEN) {
        return false;
    }
    while let Some(after) = rest.strip_prefix(REMINDER_OPEN) {
        let Some((_, body)) = after.split_once('>') else {
            return false;
        };
        let Some((_, tail)) = body.split_once(REMINDER_CLOSE) else {
            return false;
        };
        let Some((_, tail)) = tail.split_once('>') else {
            return false;
        };
        rest = tail.trim_start();
    }
    rest.is_empty()
}
