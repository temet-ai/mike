mod plan;
#[cfg(test)]
mod tests;

use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::db_provision::{Call, Runner, SystemRunner};
use crate::finding::Finding;
use crate::launch::{BackendId, Check, ProbeReport};
use crate::tui::screens::setup::facts::Tool;
use crate::tui::screens::setup::panel::{Panel, PanelEvent};
use crate::tui::screens::setup::persist::persist_env_var_at;
use crate::tui::screens::setup::step::{Cx, Hints, Outcome, WizardStep};
use plan::{
    BackendSource, HERDR_SITE, Probed, TermAction, TermInputs, installed, plan_panel, real_source,
};

const ENV_KEY: &str = "MIKE_LAUNCH_BACKEND";

pub(in crate::tui::screens::setup) struct Terminal {
    backends: BackendSource,
    panel: Panel<TermAction>,
}

fn first_remedy(report: &ProbeReport) -> String {
    report
        .checks
        .iter()
        .find(|c| !c.passed)
        .and_then(|c| c.remedy.clone())
        .unwrap_or_default()
}

fn check_finding(check: &Check) -> Finding {
    if check.passed {
        return Finding::ok(format!("{}: {}", check.name, check.detail));
    }
    match &check.remedy {
        Some(remedy) => {
            Finding::failed(format!("{}: {} · fix: {remedy}", check.name, check.detail))
        }
        None => Finding::failed(format!("{}: {}", check.name, check.detail)),
    }
}

impl Terminal {
    pub(in crate::tui::screens::setup) fn real() -> Self {
        Self::with_source(real_source())
    }

    fn with_source(backends: BackendSource) -> Self {
        Self {
            backends,
            panel: Panel::default(),
        }
    }

    async fn probe(&self, ids: &[BackendId]) -> Vec<Probed> {
        let probes = ids.iter().filter(|id| **id != BackendId::None).map(|id| {
            let id = *id;
            let backend = (self.backends)(id);
            async move {
                let backend = backend?;
                let report = backend.probe().await;
                Some(Probed {
                    id,
                    available: report.available,
                    remedy: first_remedy(&report),
                })
            }
        });
        futures::future::join_all(probes)
            .await
            .into_iter()
            .flatten()
            .collect()
    }

    async fn validate_and_save(&self, id: BackendId, cx: &mut Cx<'_>) -> Vec<Finding> {
        let file = cx.env.config_file().to_path_buf();
        if id != BackendId::None {
            let Some(backend) = (self.backends)(id) else {
                return vec![Finding::failed(format!(
                    "{id} cannot be checked here: it is not in this build, or live checks are switched off while {} is set.",
                    crate::sandbox::ENV_VAR
                ))];
            };
            let probe = backend.probe().await;
            if !probe.available {
                return probe.checks.iter().map(check_finding).collect();
            }
            let report = backend.validate().await;
            let mut findings: Vec<Finding> = report.checks.iter().map(check_finding).collect();
            if !report.passed {
                findings.push(Finding::failed(
                    "The live check failed, so nothing was saved. Fix it and run the check again, or choose another backend.",
                ));
                return findings;
            }
            findings.extend(self.save(id, &file, cx));
            return findings;
        }
        self.save(id, &file, cx)
    }

    fn save(&self, id: BackendId, file: &std::path::Path, cx: &mut Cx<'_>) -> Vec<Finding> {
        match persist_env_var_at(file, ENV_KEY, id.as_str()) {
            Ok(()) => {
                cx.config.launch.backend = id.as_str().to_string();
                vec![Finding::ok(format!(
                    "Saved launch_backend = {id} in {}.",
                    file.display()
                ))]
            }
            Err(e) => vec![Finding::failed(format!(
                "Could not write {}: {e}",
                file.display()
            ))],
        }
    }

    async fn install_herdr(&mut self, cx: &mut Cx<'_>) {
        if !cx.env.facts.has(Tool::Brew) {
            self.panel.show(vec![Finding::note(format!(
                "Nothing was run. Install herdr from {HERDR_SITE}, then run this step again."
            ))]);
            return;
        }
        let call = Call::new("brew", &["install", "herdr"]);
        match SystemRunner.run(&call).await {
            Ok(out) if out.success => {
                self.enter(cx).await;
                self.panel.show(vec![Finding::ok(format!(
                    "`{}` finished. herdr is the first choice now.",
                    call.shown()
                ))]);
            }
            Ok(out) => self.panel.record(vec![Finding::failed(format!(
                "`{}` failed: {}",
                call.shown(),
                out.detail()
            ))]),
            Err(e) => self.panel.record(vec![Finding::failed(format!(
                "`{}` could not run: {e}",
                call.shown()
            ))]),
        }
    }

    async fn execute(&mut self, cx: &mut Cx<'_>) {
        let Some(action) = self.panel.selected().map(|c| c.action) else {
            return;
        };
        match action {
            TermAction::InstallHerdr => self.install_herdr(cx).await,
            TermAction::Backend(id) => {
                let findings = self.validate_and_save(id, cx).await;
                self.panel.record(findings);
            }
        }
    }
}

#[async_trait(?Send)]
impl WizardStep for Terminal {
    fn names(&self) -> &'static [&'static str] {
        &["terminal"]
    }

    fn label(&self) -> &'static str {
        "terminal"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        let path_var = cx.env.locations.path_var.clone();
        cx.env
            .facts
            .refresh(
                &SystemRunner,
                path_var.as_deref(),
                &[
                    Tool::Herdr,
                    Tool::Tmux,
                    Tool::Hyprctl,
                    Tool::XdgTerminalExec,
                    Tool::Brew,
                ],
            )
            .await;
        let mut inputs = TermInputs {
            herdr: cx.env.facts.get(Tool::Herdr).cloned(),
            tmux: cx.env.facts.get(Tool::Tmux).cloned(),
            hyprctl: cx.env.facts.has(Tool::Hyprctl),
            xdg: cx.env.facts.has(Tool::XdgTerminalExec),
            brew: cx.env.facts.has(Tool::Brew),
            current: cx.config.launch.backend.parse::<BackendId>().ok(),
            probed: Vec::new(),
        };
        inputs.probed = self.probe(&installed(&inputs)).await;
        self.panel = plan_panel(&inputs);
    }

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        match self.panel.handle(key) {
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
        self.panel.draw(frame, area, "terminal");
    }

    fn hints(&self) -> Hints {
        self.panel.hints()
    }
}
