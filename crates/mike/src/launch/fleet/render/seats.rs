use super::clock::ago;
use super::ink::{DIM, FAINT, GREEN, INK, PURPLE, RED, Rgb, YELLOW, pad, paint, role_colour};
use crate::launch::fleet::status::label as cache_label;
use crate::launch::fleet::status::{FleetStatus, SeatStatus};

const WINDOW: i64 = 1_000_000;
use crate::launch::fleet::tell_context::REPLACE_AT;
const WARN_AT: i64 = 400_000;
const CELLS: usize = 20;
const SHOWN_WHEN_ALL_CLOSED: usize = 8;
const SEAT_WIDTH: usize = 24;
const ROLE_WIDTH: usize = 20;
const MODEL_WIDTH: usize = 8;
const STATE_WIDTH: usize = 9;
const CONTEXT_WIDTH: usize = 46;
const REQUESTS_WIDTH: usize = 10;
const LAST_REQUEST_WIDTH: usize = 14;

pub(super) fn seats(status: &FleetStatus) -> Vec<String> {
    let (shown, hidden) = visible_seats(&status.seats);
    let mut lines = vec![format!(
        " {}{}",
        paint("SEATS", PURPLE, true),
        paint(
            "   context bar: │ marks 600k, where a seat is replaced",
            FAINT,
            false
        )
    )];
    lines.push(format!(
        "  {}",
        paint(
            &format!(
                "{}{}{}{}{}{}{}first request",
                pad("seat", SEAT_WIDTH),
                pad("role", ROLE_WIDTH),
                pad("model", MODEL_WIDTH),
                pad("state", STATE_WIDTH),
                pad("context", CONTEXT_WIDTH),
                pad("requests", REQUESTS_WIDTH),
                pad("last request", LAST_REQUEST_WIDTH),
            ),
            DIM,
            false
        )
    ));
    lines.extend(shown.iter().map(|seat| row(seat, status)));
    if hidden > 0 {
        let noun = if hidden == 1 { "seat" } else { "seats" };
        lines.push(format!(
            "  {}",
            paint(&format!("{hidden} closed {noun} not shown"), FAINT, false)
        ));
    }
    lines
}

fn visible_seats(seats: &[SeatStatus]) -> (Vec<&SeatStatus>, usize) {
    let open: Vec<&SeatStatus> = seats.iter().filter(|s| !s.closed).collect();
    if !open.is_empty() {
        let hidden = seats.len() - open.len();
        return (open, hidden);
    }
    let from = seats.len().saturating_sub(SHOWN_WHEN_ALL_CLOSED);
    (seats[from..].iter().collect(), from)
}

fn row(seat: &SeatStatus, status: &FleetStatus) -> String {
    let name_colour = role_colour(&seat.role).unwrap_or(INK);
    let requests = seat.requests.to_string();
    format!(
        "  {}{}{}{}{}{}{}{}",
        pad(&paint(&seat.seat, name_colour, true), SEAT_WIDTH),
        pad(&paint(&seat.role, DIM, false), ROLE_WIDTH),
        pad(&paint(&seat.model, DIM, false), MODEL_WIDTH),
        pad(
            &paint(&seat.state, state_colour(&seat.state), false),
            STATE_WIDTH
        ),
        pad(&context_bar(seat.context_tokens), CONTEXT_WIDTH),
        pad(&paint(&requests, INK, false), REQUESTS_WIDTH),
        pad(
            &paint(&ago(seat.last_request_at, status.now), DIM, false),
            LAST_REQUEST_WIDTH
        ),
        cache_start(seat)
    )
}

fn cache_start(seat: &SeatStatus) -> String {
    match seat.first_request_cache {
        Some(cache) if cache.cold => paint(&cache_label(Some(&cache)), RED, true),
        Some(cache) => paint(&cache_label(Some(&cache)), FAINT, false),
        None => String::new(),
    }
}

fn state_colour(state: &str) -> Rgb {
    match state {
        "working" => GREEN,
        "idle" => YELLOW,
        "blocked" | "silent" | "quota" => RED,
        "closed" => FAINT,
        _ => DIM,
    }
}

pub(super) fn context_bar(tokens: Option<i64>) -> String {
    let Some(tokens) = tokens else {
        return format!(
            "{}{}",
            paint(&"·".repeat(CELLS), FAINT, false),
            paint("      no requests", FAINT, false)
        );
    };
    let tokens = tokens.max(0);
    let colour = if tokens < WARN_AT {
        GREEN
    } else if tokens < REPLACE_AT {
        YELLOW
    } else {
        RED
    };
    let filled = cell_for(tokens).min(CELLS);
    let mark = cell_for(REPLACE_AT);
    let cells: String = (0..CELLS)
        .map(|i| {
            if i == mark {
                paint("▐", if i < filled { RED } else { INK }, true)
            } else if i < filled {
                paint("█", colour, false)
            } else {
                paint("░", FAINT, false)
            }
        })
        .collect();
    let number = paint(
        &format!(" {:>4}k of {}k", (tokens + 500) / 1000, WINDOW / 1000),
        colour,
        false,
    );
    let replace = if tokens >= REPLACE_AT {
        paint("  replace", RED, true)
    } else {
        String::new()
    };
    format!("{cells}{number}{replace}")
}

fn cell_for(tokens: i64) -> usize {
    let cells = i64::try_from(CELLS).unwrap_or(i64::MAX);
    usize::try_from((tokens * cells + WINDOW / 2) / WINDOW).unwrap_or(0)
}
