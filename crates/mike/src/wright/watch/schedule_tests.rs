use std::time::Duration;

use tokio::time::Instant;

use super::{Pass, Schedule, TICK, TOGGLE_POLL};

fn schedule(next_regular: Instant) -> Schedule {
    Schedule {
        events: None,
        next_regular,
        event_due: None,
    }
}

#[tokio::test]
async fn wake_while_disabled_rechecks_the_toggle_within_the_poll() {
    let idle = schedule(Instant::now() + TICK);
    let wake = idle.wake(false);
    assert!(wake <= Instant::now() + TOGGLE_POLL);
}

#[tokio::test]
async fn wake_while_enabled_never_sleeps_past_the_toggle_poll() {
    let idle = schedule(Instant::now() + TICK);
    let wake = idle.wake(true);
    assert!(wake <= Instant::now() + TOGGLE_POLL);
}

#[tokio::test]
async fn restart_on_enable_makes_a_regular_pass_due_at_once() {
    let start = Instant::now();
    let mut idle = schedule(start + TICK);
    idle.event_due = Some(start + Duration::from_secs(1));
    idle.restart(true);
    assert_eq!(idle.event_due, None);
    assert_eq!(idle.due(Instant::now()), Some(Pass::Regular));
}

#[tokio::test]
async fn restart_on_disable_drops_a_pending_event_pass() {
    let start = Instant::now();
    let mut idle = schedule(start + TICK);
    idle.event_due = Some(start);
    idle.restart(false);
    assert_eq!(idle.due(Instant::now()), None);
}
