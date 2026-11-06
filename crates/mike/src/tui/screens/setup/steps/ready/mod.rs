#[cfg(test)]
mod tests;
mod view;

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::claude_code::{FUNCTION_KEY, read_setting};
use crate::config::ProxyConfig;
use crate::doctor::Report;
use crate::finding::Finding;
use crate::tui::client::FirstRequest;
use crate::tui::screens::Nav;
use crate::tui::screens::setup::report::findings_from;
use crate::tui::screens::setup::step::{Cx, Hints, Outcome, WizardStep};

const WEB_UI: &str = "http://localhost:9990/";
const TOUR_URL: &str = "http://localhost:9990/?tour=1";
const POLL_EVERY: Duration = Duration::from_secs(2);

pub(super) type ReportFn =
    for<'a> fn(&'a ProxyConfig) -> Pin<Box<dyn Future<Output = Report> + 'a>>;
type Opener = fn(&str);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Wait {
    Waiting,
    Stopped,
    Arrived(FirstRequest),
}

fn real_report(config: &ProxyConfig) -> Pin<Box<dyn Future<Output = Report> + '_>> {
    Box::pin(crate::doctor::build_report(config))
}

pub(in crate::tui::screens::setup) fn open_url(url: &str) {
    if crate::sandbox::active() {
        return;
    }
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

pub(in crate::tui::screens::setup) struct Ready {
    report: ReportFn,
    open: Opener,
    poll_every: Duration,
    started: DateTime<Utc>,
    last_poll: Option<Instant>,
    findings: Vec<Finding>,
    launch: String,
    wait: Wait,
}

impl Ready {
    pub(in crate::tui::screens::setup) fn new() -> Self {
        Self::with_parts(real_report, open_url, POLL_EVERY)
    }

    fn with_parts(report: ReportFn, open: Opener, poll_every: Duration) -> Self {
        Self {
            report,
            open,
            poll_every,
            started: Utc::now(),
            last_poll: None,
            findings: Vec::new(),
            launch: String::new(),
            wait: Wait::Waiting,
        }
    }

    async fn look(&mut self, cx: &mut Cx<'_>) {
        self.started = Utc::now();
        self.last_poll = None;
        self.wait = Wait::Waiting;
        self.findings = findings_from(&(self.report)(cx.config).await);
        let function = read_setting(cx.env.config_file(), FUNCTION_KEY)
            .ok()
            .flatten();
        self.launch = match function {
            Some(name) => format!(
                "Start Claude Code in any folder with `mike claude`, or with `{name}`, the function setup wrote into your shell. Open a new terminal first if it is not found."
            ),
            None => "Start Claude Code in any folder with `mike claude`.".to_string(),
        };
    }

    async fn poll(&mut self, cx: &Cx<'_>) {
        if self.wait != Wait::Waiting {
            return;
        }
        if self
            .last_poll
            .is_some_and(|at| at.elapsed() < self.poll_every)
        {
            return;
        }
        self.last_poll = Some(Instant::now());
        if let Some(request) = cx.client.first_request_since(self.started).await {
            self.wait = Wait::Arrived(request);
        }
    }
}

#[async_trait(?Send)]
impl WizardStep for Ready {
    fn names(&self) -> &'static [&'static str] {
        &["ready"]
    }

    fn label(&self) -> &'static str {
        "ready"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        self.look(cx).await;
    }

    async fn tick(&mut self, cx: &mut Cx<'_>) {
        self.poll(cx).await;
    }

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        match key.code {
            KeyCode::Enter => return Outcome::Leave(Nav::LaunchClaude),
            KeyCode::Left => return Outcome::Back,
            KeyCode::Char('b') => (self.open)(WEB_UI),
            KeyCode::Char('t') => (self.open)(TOUR_URL),
            KeyCode::Char('c') if self.wait == Wait::Waiting => self.wait = Wait::Stopped,
            KeyCode::Char('r') => self.look(cx).await,
            _ => {}
        }
        Outcome::Stay
    }

    fn draw(&self, frame: &mut Frame, area: Rect) {
        view::draw(frame, area, &self.launch, &self.wait, &self.findings);
    }

    fn hints(&self) -> Hints {
        let last = match self.wait {
            Wait::Waiting => ("c", "stop"),
            Wait::Stopped | Wait::Arrived(_) => ("r", "again"),
        };
        vec![("⏎", "claude"), ("b", "web"), ("t", "tour"), last]
    }
}
