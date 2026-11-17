use chrono::{DateTime, Duration, Utc};

use crate::store::persistence::conditions::{Fired, SessionPresence};

pub const RESUME_WINDOW_MINUTES: i64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Fire(Fired),
    Wait,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnVia {
    Mail,
    Resume,
    Start,
}

pub fn decide(held: bool, deadline: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Decision {
    if held {
        return Decision::Fire(Fired::Held);
    }
    match deadline {
        Some(deadline) if deadline <= now => Decision::Fire(Fired::Missed),
        _ => Decision::Wait,
    }
}

pub fn choose_return(presence: Option<&SessionPresence>, now: DateTime<Utc>) -> ReturnVia {
    let Some(presence) = presence else {
        return ReturnVia::Start;
    };
    if presence.live {
        return ReturnVia::Mail;
    }
    let recent = now - presence.last_request_at <= Duration::minutes(RESUME_WINDOW_MINUTES);
    if recent && presence.cwd.is_some() {
        ReturnVia::Resume
    } else {
        ReturnVia::Start
    }
}

#[cfg(test)]
#[path = "decide_tests.rs"]
mod decide_tests;
