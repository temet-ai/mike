use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::super::panel::{Choice, Panel, PanelEvent};
use super::super::step::{Cx, Hints, Outcome, WizardStep};
use crate::finding::Finding;
use crate::service::{
    Collected, Observation, install_for_setup, observe_for_setup, restart_for_setup,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum ServiceAction {
    Install,
    Check,
    Restart,
    Leave,
}

pub(in super::super) struct ServiceInputs {
    pub(in super::super) observation: Option<Observation>,
    pub(in super::super) proxy_answers: bool,
    pub(in super::super) linux: bool,
}

fn found_lines(inputs: &ServiceInputs) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(observation) = &inputs.observation {
        lines.push(format!("Service manager: {}", observation.manager_name));
        lines.extend(observation.unit_lines.iter().cloned());
    }
    lines.push(if inputs.proxy_answers {
        "A mike proxy answers on the inspect address".to_string()
    } else {
        "Nothing answers on the inspect address".to_string()
    });
    lines
}

fn install_choice(observation: &Observation, linux: bool) -> Choice<ServiceAction> {
    let (kind, names) = if linux {
        (
            "systemd user units",
            "mike.service and mike-forward.service",
        )
    } else {
        (
            "launchd agents",
            "ai.temet.mike.proxy and ai.temet.mike.forward",
        )
    };
    let mut choice = Choice::new(
        ServiceAction::Install,
        "Install the background service",
        format!(
            "Writes two {kind} ({names}) that run this mike binary, start at login and restart after a crash, then waits until the proxy and the sidecar answer. It refuses when something already answers on mike's ports."
        ),
    )
    .recommended()
    .command("mike service install");
    if linux && !observation.installed {
        choice = choice.root_command("sudo loginctl enable-linger $USER");
    }
    choice
}

pub(in super::super) fn plan_panel(inputs: &ServiceInputs) -> Panel<ServiceAction> {
    let found = found_lines(inputs);
    let Some(observation) = &inputs.observation else {
        return Panel::new(found, Vec::new()).with_note(
            "Background services are supported on macOS (launchd) and Linux (systemd), so there is nothing to install on this system.",
        );
    };
    if let Some(units) = &observation.dev_stack {
        return Panel::new(found, Vec::new()).with_note(format!(
            "The developer stack is running here ({units}). A background service would fight it for the same ports and the same database, so this step does nothing."
        ));
    }
    let leave = Choice::new(
        ServiceAction::Leave,
        "Do not install a service",
        "`mike claude` starts the proxy by itself when it is down. Nothing keeps running after you log out.",
    );
    let choices = if observation.installed {
        vec![
            Choice::new(
                ServiceAction::Check,
                "Check the installed service",
                "Reads the state of both units and asks the proxy for its health.",
            )
            .recommended()
            .command("mike service status"),
            Choice::new(
                ServiceAction::Restart,
                "Restart the installed service",
                "Restarts both units and waits until the proxy reports this binary's build.",
            )
            .command("mike service restart"),
            leave,
        ]
    } else {
        vec![install_choice(observation, inputs.linux), leave]
    };
    Panel::new(found, choices)
}

fn findings_from(collected: Collected) -> Vec<Finding> {
    let mut findings: Vec<Finding> = collected.lines.into_iter().map(Finding::ok).collect();
    match collected.error {
        Some(why) => findings.push(Finding::failed(why)),
        None => findings.push(Finding::ok("Both services answered.")),
    }
    findings
}

pub(in super::super) struct Service {
    panel: Panel<ServiceAction>,
}

impl Service {
    pub(in super::super) fn new() -> Self {
        Self {
            panel: Panel::default(),
        }
    }

    async fn execute(&mut self, cx: &mut Cx<'_>) {
        let Some(action) = self.panel.selected().map(|c| c.action) else {
            return;
        };
        let findings = match action {
            ServiceAction::Install => findings_from(install_for_setup(cx.config).await),
            ServiceAction::Restart => findings_from(restart_for_setup(cx.config).await),
            ServiceAction::Check => check(cx).await,
            ServiceAction::Leave => vec![Finding::note(
                "No service was installed. Run `mike service install` later to add one.",
            )],
        };
        self.panel.record(findings);
    }
}

async fn check(cx: &Cx<'_>) -> Vec<Finding> {
    let mut findings = Vec::new();
    match observe_for_setup().await {
        Some(observation) => {
            for line in observation.unit_lines {
                findings.push(Finding::ok(line));
            }
            if !observation.running {
                findings.push(Finding::failed("The units are not both running."));
            }
        }
        None => findings.push(Finding::failed("The service manager could not be read.")),
    }
    findings.push(if cx.client.health().await {
        Finding::ok("The proxy answered its health check.")
    } else {
        Finding::failed("The proxy did not answer its health check.")
    });
    findings
}

#[async_trait(?Send)]
impl WizardStep for Service {
    fn names(&self) -> &'static [&'static str] {
        &["service"]
    }

    fn label(&self) -> &'static str {
        "service"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        let inputs = ServiceInputs {
            observation: observe_for_setup().await,
            proxy_answers: cx.client.health().await,
            linux: std::env::consts::OS == "linux",
        };
        self.panel = plan_panel(&inputs);
    }

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        match self.panel.handle(key) {
            PanelEvent::Run if self.panel.choices.is_empty() => Outcome::Next,
            PanelEvent::Run => {
                self.execute(cx).await;
                Outcome::Stay
            }
            PanelEvent::Next | PanelEvent::Skip => Outcome::Next,
            PanelEvent::Back => Outcome::Back,
            PanelEvent::Recheck => {
                self.enter(cx).await;
                Outcome::Stay
            }
            PanelEvent::FinishLater => Outcome::FinishLater,
            PanelEvent::Idle => Outcome::Stay,
        }
    }

    fn draw(&self, frame: &mut Frame, area: Rect) {
        self.panel.draw(frame, area, "background service");
    }

    fn hints(&self) -> Hints {
        if self.panel.choices.is_empty() {
            return vec![("⏎", "next"), ("←", "back"), ("f", "later")];
        }
        self.panel.hints()
    }
}
