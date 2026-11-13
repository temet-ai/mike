use uuid::Uuid;

use super::status::{FleetStatus, SeatStatus};
use super::tell::Notice;

pub const REPLACE_AT: i64 = 600_000;
const CONTROL_SEATS: [&str; 4] = ["coordinator", "acceptance", "observer", "reference"];

fn is_control(seat: &SeatStatus) -> bool {
    CONTROL_SEATS.contains(&seat.role.as_str())
        || super::seat_settings::profile_for(&seat.seat) != "worker"
}

fn key(seat: &SeatStatus) -> String {
    match seat.session {
        Some(session) => format!("context-handoff:{}:{session}", seat.seat),
        None => format!("context-handoff:{}", seat.seat),
    }
}

pub fn past_the_line(
    fleet: &str,
    coordinator: Uuid,
    status: &FleetStatus,
    said: impl Fn(&str) -> bool,
) -> Vec<Notice> {
    status
        .seats
        .iter()
        .filter(|s| !s.closed && is_control(s))
        .filter_map(|s| Some((s, s.context_tokens.filter(|t| *t >= REPLACE_AT)?)))
        .filter(|(s, _)| !said(&key(s)))
        .map(|(s, tokens)| Notice {
            to: coordinator,
            key: key(s),
            covers: Vec::new(),
            body: format!(
                "[mike context] The {seat} seat's context is {k}k tokens, past the {line}k line where a control seat is replaced: every request re-reads all of it. Start a successor for {seat} now. It reads the seat's register and findings from notes, not from this session: list_session_notes with the tag fleet:{fleet} and name {seat} (the acceptance seat's obligations are `task` notes tagged acceptance-obligation, its verdicts `decision` notes tagged acceptance). Close the old {seat} session once its notes are current. mike mails this once per recorded session, or seat when its session is unknown.",
                seat = s.seat,
                k = (tokens + 500) / 1000,
                line = REPLACE_AT / 1000,
            ),
        })
        .collect()
}

#[cfg(test)]
#[path = "tell_context_tests.rs"]
pub mod tests;
