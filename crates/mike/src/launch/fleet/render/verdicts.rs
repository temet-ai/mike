use super::clock::hour_and_minute;
use super::ink::{FAINT, GREEN, INK, ORANGE, PURPLE, RED, paint};
use crate::launch::fleet::status::FleetStatus;

pub(super) fn verdicts(status: &FleetStatus) -> Vec<String> {
    let mut lines = vec![format!(
        " {}",
        paint("LATEST FROM ACCEPTANCE", PURPLE, true)
    )];
    for entry in &status.verdicts {
        let colour = if entry.verdict == "pass" { GREEN } else { RED };
        lines.push(format!(
            "  {}  {}  {}",
            paint(&hour_and_minute(entry.at), FAINT, false),
            paint(&entry.verdict.to_uppercase(), colour, true),
            paint(&entry.title, INK, false)
        ));
    }
    if status.verdicts.is_empty() {
        lines.push(format!("  {}", paint("nothing judged yet", FAINT, false)));
    }
    lines
}

pub(super) fn blocked(status: &FleetStatus) -> Vec<String> {
    if status.blocked.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![format!(" {}", paint("BLOCKED", RED, true))];
    lines.extend(status.blocked.iter().map(|entry| {
        format!(
            "  {}  {}",
            paint(&entry.seat, ORANGE, false),
            paint(&entry.title, INK, false)
        )
    }));
    lines
}
