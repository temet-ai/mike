use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};

pub(crate) const DOWN_AFTER_CONSECUTIVE_READINGS: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CaptureOutageEvent {
    Down {
        since: DateTime<Utc>,
        reason: String,
    },
    Recovered {
        since: DateTime<Utc>,
    },
}

impl CaptureOutageEvent {
    pub(crate) fn title(&self) -> &'static str {
        match self {
            CaptureOutageEvent::Down { .. } => "mike: captures are not being saved",
            CaptureOutageEvent::Recovered { .. } => "mike: captures are being saved again",
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            CaptureOutageEvent::Down { since, reason } => format!(
                "mike cannot save captures since {} ({reason}). Claude keeps working and no request is refused, \
                 but requests sent from now on are not recorded until this is fixed. See docs/CAPTURE_OUTAGES.md.",
                since.format("%Y-%m-%d %H:%M UTC")
            ),
            CaptureOutageEvent::Recovered { since } => format!(
                "mike saves captures again. Requests sent after {} and before now were not recorded.",
                since.format("%Y-%m-%d %H:%M UTC")
            ),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct CaptureOutageTracker {
    since: Option<DateTime<Utc>>,
    readings: u32,
    announced: bool,
}

impl CaptureOutageTracker {
    pub(crate) fn observe(
        &mut self,
        failure: Option<&str>,
        now: DateTime<Utc>,
    ) -> Option<CaptureOutageEvent> {
        match failure {
            Some(reason) => self.observe_failure(reason, now),
            None => self.observe_healthy(),
        }
    }

    fn observe_failure(&mut self, reason: &str, now: DateTime<Utc>) -> Option<CaptureOutageEvent> {
        let since = *self.since.get_or_insert(now);
        self.readings = self.readings.saturating_add(1);
        if self.announced || self.readings < DOWN_AFTER_CONSECUTIVE_READINGS {
            return None;
        }
        self.announced = true;
        Some(CaptureOutageEvent::Down {
            since,
            reason: reason.to_owned(),
        })
    }

    fn observe_healthy(&mut self) -> Option<CaptureOutageEvent> {
        let finished = std::mem::take(self);
        let since = finished.since?;
        finished
            .announced
            .then_some(CaptureOutageEvent::Recovered { since })
    }
}

static TRACKER: OnceLock<Mutex<CaptureOutageTracker>> = OnceLock::new();

pub(crate) fn note_reading(failure: Option<&str>) {
    let tracker = TRACKER.get_or_init(Mutex::default);
    let event = tracker
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .observe(failure, Utc::now());
    if let Some(event) = event {
        announce(&event);
    }
}

#[cfg(any(test, feature = "test-seams"))]
static TEST_SINK: OnceLock<Mutex<std::sync::mpsc::Sender<String>>> = OnceLock::new();

#[cfg(any(test, feature = "test-seams"))]
pub(crate) fn install_test_sink() -> std::sync::mpsc::Receiver<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = TEST_SINK.set(Mutex::new(tx));
    rx
}

#[cfg(any(test, feature = "test-seams"))]
fn deliver_to_test_sink(message: &str) -> bool {
    let Some(sink) = TEST_SINK.get() else {
        return false;
    };
    let _ = sink
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .send(message.to_owned());
    true
}

#[cfg(not(any(test, feature = "test-seams")))]
fn deliver_to_test_sink(_message: &str) -> bool {
    false
}

fn announce(event: &CaptureOutageEvent) {
    let (urgency, message) = (urgency(event), event.message());
    log_notice(event, &message);
    if deliver_to_test_sink(&message) {
        return;
    }
    let title = event.title();
    std::thread::spawn(move || deliver_desktop(urgency, title, &message));
}

fn log_notice(event: &CaptureOutageEvent, message: &str) {
    match event {
        CaptureOutageEvent::Down { .. } => log_down(message),
        CaptureOutageEvent::Recovered { .. } => log_recovered(message),
    }
}

fn log_down(message: &str) {
    tracing::error!(target: "mike::operator_notice", "{message}");
}

fn log_recovered(message: &str) {
    tracing::warn!(target: "mike::operator_notice", "{message}");
}

fn urgency(event: &CaptureOutageEvent) -> &'static str {
    match event {
        CaptureOutageEvent::Down { .. } => "critical",
        CaptureOutageEvent::Recovered { .. } => "normal",
    }
}

fn deliver_desktop(urgency: &str, title: &str, message: &str) {
    let Some(program) = ["omarchy-notification-send", "notify-send"]
        .into_iter()
        .find(|p| on_path(p))
    else {
        return;
    };
    if let Err(e) = std::process::Command::new(program)
        .args([&format!("--urgency={urgency}"), title, message])
        .status()
    {
        tracing::warn!(program, error = %e, "capture outage notice: desktop notification failed");
    }
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

#[cfg(test)]
#[path = "capture_outage_tests.rs"]
mod capture_outage_tests;
