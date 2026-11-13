use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use chrono::{DateTime, Utc};

use super::usage::{LlmRole, ROLE_COUNT};

#[cfg(test)]
#[path = "outage_tests.rs"]
pub mod outage_tests;

pub const DOWN_AFTER: Duration = Duration::from_secs(15 * 60);
const PENDING_CAP: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Down,
    Recovered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorNotice {
    pub role: LlmRole,
    pub kind: NoticeKind,
    pub failing_since: DateTime<Utc>,
    pub at: DateTime<Utc>,
    pub failures: u32,
    pub last_error: String,
}

impl OperatorNotice {
    pub fn message(&self) -> String {
        let role = self.role.label();
        let since = self.failing_since.format("%Y-%m-%d %H:%M UTC");
        match self.kind {
            NoticeKind::Down => format!(
                "Every background model call for the {role} worker has failed since {since} ({} in a row). Last error: {}",
                self.failures, self.last_error
            ),
            NoticeKind::Recovered => format!(
                "The {role} worker's model calls work again. They had been failing since {since} ({} failures).",
                self.failures
            ),
        }
    }
}

pub trait NoticeSink: Send + Sync {
    fn deliver(&self, notice: &OperatorNotice);
}

#[derive(Debug, Clone, Default)]
struct RoleRun {
    failing_since: Option<DateTime<Utc>>,
    failures: u32,
    announced: bool,
}

#[derive(Default)]
pub struct OutageTracker {
    roles: Mutex<[RoleRun; ROLE_COUNT]>,
}

impl OutageTracker {
    pub fn record_failure(
        &self,
        role: LlmRole,
        error: &str,
        now: DateTime<Utc>,
    ) -> Option<OperatorNotice> {
        let mut roles = self.roles.lock().unwrap_or_else(|e| e.into_inner());
        let run = roles.get_mut(role.index())?;
        let since = *run.failing_since.get_or_insert(now);
        run.failures = run.failures.saturating_add(1);
        let silent_for = (now - since).to_std().unwrap_or_default();
        if run.announced || silent_for < DOWN_AFTER {
            return None;
        }
        run.announced = true;
        Some(OperatorNotice {
            role,
            kind: NoticeKind::Down,
            failing_since: since,
            at: now,
            failures: run.failures,
            last_error: error.to_owned(),
        })
    }

    pub fn record_success(
        &self,
        role: LlmRole,
        now: DateTime<Utc>,
    ) -> Option<OperatorNotice> {
        let mut roles = self.roles.lock().unwrap_or_else(|e| e.into_inner());
        let run = roles.get_mut(role.index())?;
        let finished = std::mem::take(run);
        let since = finished.failing_since?;
        finished.announced.then(|| OperatorNotice {
            role,
            kind: NoticeKind::Recovered,
            failing_since: since,
            at: now,
            failures: finished.failures,
            last_error: String::new(),
        })
    }
}

static TRACKER: OnceLock<OutageTracker> = OnceLock::new();
static SINK: OnceLock<Arc<dyn NoticeSink>> = OnceLock::new();
static PENDING: Mutex<VecDeque<OperatorNotice>> = Mutex::new(VecDeque::new());

fn tracker() -> &'static OutageTracker {
    TRACKER.get_or_init(OutageTracker::default)
}

pub fn install_sink(sink: Arc<dyn NoticeSink>) -> bool {
    SINK.set(sink).is_ok()
}

pub fn drain_pending() -> Vec<OperatorNotice> {
    let mut queue = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    queue.drain(..).collect()
}

fn log_down(message: &str) {
    tracing::error!(target: "mike::operator_notice", "{message}");
}

fn log_recovered(message: &str) {
    tracing::warn!(target: "mike::operator_notice", "{message}");
}

fn log_notice(notice: &OperatorNotice) {
    let message = notice.message();
    match notice.kind {
        NoticeKind::Down => log_down(&message),
        NoticeKind::Recovered => log_recovered(&message),
    }
}

fn deliver_notice(notice: &OperatorNotice) {
    match SINK.get() {
        Some(sink) => sink.deliver(notice),
        None => DesktopSink.deliver(notice),
    }
}

fn queue_notice(notice: OperatorNotice) {
    let mut queue = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    if queue.len() >= PENDING_CAP {
        queue.pop_front();
    }
    queue.push_back(notice);
}

fn emit(notice: OperatorNotice) {
    log_notice(&notice);
    deliver_notice(&notice);
    queue_notice(notice);
}

pub fn note_failure(role: LlmRole, error: &str) {
    if let Some(notice) = tracker().record_failure(role, error, Utc::now()) {
        emit(notice);
    }
}

pub fn note_success(role: LlmRole) {
    if let Some(notice) = tracker().record_success(role, Utc::now()) {
        emit(notice);
    }
}

pub struct DesktopSink;

impl DesktopSink {
    fn command(notice: &OperatorNotice) -> Option<(&'static str, Vec<String>)> {
        let title = match notice.kind {
            NoticeKind::Down => "mike: background model calls are failing",
            NoticeKind::Recovered => "mike: background model calls recovered",
        };
        let urgency = match notice.kind {
            NoticeKind::Down => "critical",
            NoticeKind::Recovered => "normal",
        };
        let program = ["omarchy-notification-send", "notify-send"]
            .into_iter()
            .find(|p| on_path(p))?;
        Some((
            program,
            vec![
                format!("--urgency={urgency}"),
                title.to_owned(),
                notice.message(),
            ],
        ))
    }
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

impl NoticeSink for DesktopSink {
    fn deliver(&self, notice: &OperatorNotice) {
        let Some((program, args)) = Self::command(notice) else {
            return;
        };
        std::thread::spawn(move || {
            if let Err(e) = std::process::Command::new(program).args(args).status() {
                tracing::warn!(program, error = %e, "operator notice: desktop notification failed");
            }
        });
    }
}
