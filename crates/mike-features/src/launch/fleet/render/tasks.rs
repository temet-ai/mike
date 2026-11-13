use super::clock::ago;
use super::ink::{BLUE, DIM, FAINT, GREEN, INK, PURPLE, RED, Rgb, pad, paint, role_colour};
use crate::launch::fleet::status::{FleetStatus, TaskStatus, Verdict};
use chrono::{DateTime, Utc};

const ID_WIDTH: usize = 5;
const NAME_WIDTH: usize = 26;
const STATE_WIDTH: usize = 10;
const DISCIPLINE_WIDTH: usize = 20;
const CRITERIA_WIDTH: usize = 10;
const SEAT_WIDTH: usize = 24;
const COUNTS_WIDTH: usize = 13;
const ROUNDS_SHOWN_FROM: usize = 2;

pub fn is_passed(state: &str) -> bool {
    state == "passed"
}

pub fn tasks(status: &FleetStatus) -> Vec<String> {
    let mut lines = vec![format!(" {}", paint("TASKS", PURPLE, true))];
    lines.extend(status.tasks.iter().map(|t| row(t, status.now)));
    lines
}

fn row(task: &TaskStatus, now: DateTime<Utc>) -> String {
    let (mark, colour) = mark_for(&task.state);
    let discipline = role_colour(&task.discipline).unwrap_or(DIM);
    let seat = task.seat.as_deref().unwrap_or("no seat yet");
    let rounds = if task.rounds >= ROUNDS_SHOWN_FROM {
        format!(" {} rounds", task.rounds)
    } else {
        String::new()
    };
    let counts = format!("{}✔ {}✘{rounds}", task.passes, task.fails);
    format!(
        "  {} {}{}{}{}{}{}{}{}",
        paint(mark, colour, true),
        pad(&paint(&task.id, INK, true), ID_WIDTH),
        pad(&paint(&task.name, INK, false), NAME_WIDTH),
        pad(&paint(&task.state, colour, false), STATE_WIDTH),
        pad(
            &paint(&task.discipline, discipline, false),
            DISCIPLINE_WIDTH
        ),
        pad(&paint(&task.criteria.join(","), DIM, false), CRITERIA_WIDTH),
        pad(&paint(seat, DIM, false), SEAT_WIDTH),
        pad(&paint(&counts, DIM, false), COUNTS_WIDTH),
        newest(task.last.as_ref(), now)
    )
}

fn mark_for(state: &str) -> (&'static str, Rgb) {
    match state {
        "passed" => ("✔", GREEN),
        "rework" => ("↻", RED),
        "ready" => ("◇", BLUE),
        "working" => ("●", BLUE),
        "queued" => ("○", DIM),
        _ => ("•", INK),
    }
}

fn newest(last: Option<&Verdict>, now: DateTime<Utc>) -> String {
    let Some(verdict) = last else {
        return paint("no verdict yet", FAINT, false);
    };
    let passed = verdict.verdict == "pass";
    let (word_colour, title_colour) = if passed { (GREEN, DIM) } else { (RED, INK) };
    format!(
        "{} {}{}",
        paint(&verdict.verdict.to_uppercase(), word_colour, true),
        paint(&verdict.title, title_colour, false),
        paint(&format!("  {}", ago(Some(verdict.at), now)), FAINT, false)
    )
}
