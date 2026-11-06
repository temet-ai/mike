use std::path::PathBuf;

use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::common::{base_outcome, failure};
use crate::claude_code::{Route, choose_route, read_setting};
use crate::finding::Finding;
use crate::launch::cli::{ca_cert_candidates, resolve_ca_cert, sidecar_reachable};
use crate::tui::screens::setup::panel::{Choice, Panel, PanelEvent};
use crate::tui::screens::setup::step::{Cx, Hints, Outcome, WizardStep};

pub(super) struct RouteInputs {
    pub(super) stored: Option<Route>,
    pub(super) sidecar_url: String,
    pub(super) sidecar: Result<u16, String>,
    pub(super) ca: Option<PathBuf>,
}

fn found_lines(inputs: &RouteInputs) -> Vec<String> {
    let mut lines = vec![match &inputs.sidecar {
        Ok(status) => format!(
            "The sidecar answers on {} (HTTP {status})",
            inputs.sidecar_url
        ),
        Err(why) => format!(
            "The sidecar does not answer on {} ({why})",
            inputs.sidecar_url
        ),
    }];
    lines.push(match &inputs.ca {
        Some(path) => format!(
            "The sidecar's certificate authority file exists: {}",
            path.display()
        ),
        None => "The sidecar's certificate authority file does not exist yet".to_string(),
    });
    lines.push(match inputs.stored {
        Some(route) => format!("Saved route: {}", route.as_str()),
        None => "No route is saved yet".to_string(),
    });
    lines
}

pub(super) fn plan_panel(inputs: &RouteInputs) -> Panel<Route> {
    let choices = vec![
        Choice::new(
            Route::Sidecar,
            "Through the forwarding sidecar",
            "`mike claude` and the shell function send Claude Code through the sidecar, so mike also captures the Remote Control channel that the Claude mobile and web apps use. The sidecar needs its own certificate authority file, which `mike forward` creates on first start.",
        )
        .recommended()
        .command("mike claude-code route sidecar"),
        Choice::new(
            Route::Plain,
            "Straight to the proxy",
            "Claude Code gets ANTHROPIC_BASE_URL pointing at the proxy. This needs no certificate and captures the model traffic, but not Remote Control.",
        )
        .command("mike claude-code route plain"),
    ];
    let mut panel = Panel::new(found_lines(inputs), choices);
    if let Some(stored) = inputs.stored {
        panel.cursor = panel
            .choices
            .iter()
            .position(|c| c.action == stored)
            .unwrap_or(0);
    }
    panel
}

async fn check(cx: &Cx<'_>, route: Route) -> Vec<Finding> {
    let launch = &cx.config.launch;
    match route {
        Route::Sidecar => {
            let mut findings = Vec::new();
            findings.push(match sidecar_reachable(&launch.proxy_forward_url).await {
                Ok(status) => Finding::ok(format!(
                    "The sidecar answered on {} with HTTP {status}.",
                    launch.proxy_forward_url
                )),
                Err(why) => Finding::note(format!(
                    "The sidecar does not answer on {} ({why}). Install the background service in the earlier step, or run `mike forward serve`, before you start Claude Code through `mike claude`.",
                    launch.proxy_forward_url
                )),
            });
            let cwd = std::env::current_dir().ok();
            findings.push(match resolve_ca_cert(launch, cwd.as_deref()) {
                Some(path) => Finding::ok(format!("Found the certificate authority file {}.", path.display())),
                None => Finding::note(format!(
                    "No certificate authority file exists yet. `mike forward` creates one at {} on its first start.",
                    ca_cert_candidates(launch, cwd.as_deref())
                        .first()
                        .map_or_else(|| "its default place".to_string(), |p| p.display().to_string())
                )),
            });
            findings
        }
        Route::Plain => vec![Finding::ok(format!(
            "Claude Code will use ANTHROPIC_BASE_URL=http://{}.",
            crate::doctor::connect_addr(&cx.config.proxy_listen_addr)
        ))],
    }
}

pub(super) struct RouteStep {
    panel: Panel<Route>,
}

impl RouteStep {
    pub(super) fn new() -> Self {
        Self {
            panel: Panel::default(),
        }
    }

    async fn execute(&mut self, cx: &mut Cx<'_>) {
        let Some(route) = self.panel.selected().map(|c| c.action) else {
            return;
        };
        let mut findings = match choose_route(&cx.env.locations, route) {
            Ok(findings) => findings,
            Err(e) => {
                self.panel.record(failure(e));
                return;
            }
        };
        findings.extend(check(cx, route).await);
        self.panel.record(findings);
    }
}

#[async_trait(?Send)]
impl WizardStep for RouteStep {
    fn names(&self) -> &'static [&'static str] {
        &["claude"]
    }

    fn label(&self) -> &'static str {
        "claude code"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        let cwd = std::env::current_dir().ok();
        let url = cx.config.launch.proxy_forward_url.clone();
        let stored = read_setting(cx.env.config_file(), "claude_route")
            .ok()
            .flatten()
            .and_then(|text| Route::parse(&text));
        let inputs = RouteInputs {
            stored,
            sidecar: sidecar_reachable(&url).await,
            ca: resolve_ca_cert(&cx.config.launch, cwd.as_deref()),
            sidecar_url: url,
        };
        self.panel = plan_panel(&inputs);
    }

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        let event = self.panel.handle(key);
        if let Some(outcome) = base_outcome(event) {
            return outcome;
        }
        if event == PanelEvent::Run {
            self.execute(cx).await;
        } else {
            self.enter(cx).await;
        }
        Outcome::Stay
    }

    fn draw(&self, frame: &mut Frame, area: Rect) {
        self.panel
            .draw(frame, area, "claude code · how it reaches mike");
    }

    fn hints(&self) -> Hints {
        self.panel.hints()
    }
}
