use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::result_object;

pub const FIRST_WAIT: Duration = Duration::from_secs(15 * 60);
pub const LONGEST_WAIT: Duration = Duration::from_secs(2 * 60 * 60);
const USAGE_LIMIT_STATUS: i64 = 429;

#[derive(Debug, Clone, Copy)]
struct Hold {
    until: Instant,
    wait: Duration,
}

#[derive(Debug, Default)]
pub struct LimitGate {
    hold: Mutex<Option<Hold>>,
}

pub static GATE: LimitGate = LimitGate::new();

impl LimitGate {
    pub const fn new() -> Self {
        Self {
            hold: Mutex::new(None),
        }
    }

    pub fn admit(&self, now: Instant) -> Result<(), Duration> {
        let mut hold = self.hold.lock().unwrap_or_else(PoisonError::into_inner);
        match hold.as_mut() {
            None => Ok(()),
            Some(h) if now < h.until => Err(h.until - now),
            Some(h) => {
                h.until = now + h.wait;
                Ok(())
            }
        }
    }

    pub fn holding(&self, now: Instant) -> bool {
        self.hold
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some_and(|h| now < h.until)
    }

    pub fn limited(&self, now: Instant) -> Duration {
        let mut hold = self.hold.lock().unwrap_or_else(PoisonError::into_inner);
        let wait = hold
            .map_or(FIRST_WAIT, |h| h.wait.saturating_mul(2))
            .min(LONGEST_WAIT);
        *hold = Some(Hold {
            until: now + wait,
            wait,
        });
        wait
    }

    pub fn answered(&self) {
        *self.hold.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

pub fn usage_limited(stdout: &str) -> bool {
    result_object(stdout).is_some_and(|r| {
        r.get("is_error").and_then(Value::as_bool) == Some(true)
            && r.get("api_error_status").and_then(Value::as_i64) == Some(USAGE_LIMIT_STATUS)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEEKLY: &str = r#"{"type":"result","is_error":true,"api_error_status":429,"api_error_type":"rate_limit_error","api_error":"usage_limit_reached","result":"You've hit your weekly limit"}"#;

    #[test]
    fn usage_limited_429_result_is_true_other_results_false() {
        assert!(usage_limited(WEEKLY));
        assert!(!usage_limited(
            r#"{"type":"result","is_error":true,"api_error_status":404,"result":"bad model"}"#
        ));
        assert!(!usage_limited(
            r#"{"type":"result","is_error":false,"result":"ok"}"#
        ));
        assert!(!usage_limited("not json"));
    }

    #[test]
    fn holding_limited_gate_is_true_until_the_wait_ends_or_a_call_answers() {
        let gate = LimitGate::new();
        let start = Instant::now();
        assert!(!gate.holding(start));
        let wait = gate.limited(start);
        assert!(gate.holding(start));
        assert!(gate.holding(start + wait - Duration::from_secs(1)));
        assert!(!gate.holding(start + wait));
        gate.limited(start);
        gate.answered();
        assert!(!gate.holding(start));
    }

    #[test]
    fn admit_open_gate_lets_calls_through() {
        assert_eq!(LimitGate::new().admit(Instant::now()), Ok(()));
    }

    #[test]
    fn admit_after_limit_refuses_until_wait_then_lets_one_probe_through() {
        let gate = LimitGate::new();
        let now = Instant::now();
        assert_eq!(gate.limited(now), FIRST_WAIT);
        assert!(gate.admit(now + Duration::from_secs(60)).is_err());
        let probe_at = now + FIRST_WAIT;
        assert_eq!(gate.admit(probe_at), Ok(()));
        assert!(gate.admit(probe_at + Duration::from_secs(1)).is_err());
    }

    #[test]
    fn limited_again_doubles_wait_up_to_longest() {
        let gate = LimitGate::new();
        let now = Instant::now();
        let waits: Vec<Duration> = (0..6).map(|_| gate.limited(now)).collect();
        assert_eq!(waits[1], FIRST_WAIT * 2);
        assert_eq!(waits[5], LONGEST_WAIT);
    }

    #[test]
    fn answered_reopens_gate() {
        let gate = LimitGate::new();
        let now = Instant::now();
        gate.limited(now);
        gate.answered();
        assert_eq!(gate.admit(now), Ok(()));
        assert_eq!(gate.limited(now), FIRST_WAIT);
    }
}
