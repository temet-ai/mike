//! The text shape mike gives a delivered mail message, and its recognition.

pub(crate) const MAIL_LEAD: &str = "Mail from ";
pub(crate) const UNKNOWN_SENDER: &str = "an unknown sender";
pub(crate) const MIKE_SENDER: &str = "mike";

pub(crate) fn is_mail_text(text: &str) -> bool {
    text.lines().any(|line| {
        line.trim_start_matches("<system-reminder>")
            .strip_prefix(MAIL_LEAD)
            .and_then(|rest| rest.split_once(": "))
            .is_some_and(|(who, _)| {
                who.contains("session ") || who == UNKNOWN_SENDER || who == MIKE_SENDER
            })
    })
}
