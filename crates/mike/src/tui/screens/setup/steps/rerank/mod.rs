mod check;
mod install;
mod plan;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::proxy::ensure_proxy;
use crate::db_provision::{Call, Runner, SystemRunner};
use crate::finding::{Finding, all_passed};
use crate::tui::screens::setup::panel::{Panel, PanelEvent};
use crate::tui::screens::setup::step::{Cx, Hints, Outcome, WizardStep};
use check::{LOCAL_BASE, check_call, send_settings};
use install::{
    Paths, SOCKET_UNIT, Sources, enable, ensure_llama, ensure_model, render_units, write_units,
};
use plan::{Current, RerankAction, RerankInputs, plan_panel};

const PROXYD_CANDIDATES: [&str; 2] = [
    "/usr/lib/systemd/systemd-socket-proxyd",
    "/lib/systemd/systemd-socket-proxyd",
];

pub(in crate::tui::screens::setup) struct Rerank<R: Runner = SystemRunner> {
    panel: Panel<RerankAction>,
    runner: R,
    sources: Sources,
    base: String,
    proxyd_candidates: Vec<PathBuf>,
}

impl Rerank<SystemRunner> {
    pub(in crate::tui::screens::setup) fn new() -> Self {
        Self::with_parts(
            SystemRunner,
            Sources::pinned(),
            LOCAL_BASE,
            PROXYD_CANDIDATES.iter().map(PathBuf::from).collect(),
        )
    }
}

async fn current_settings(cx: &Cx<'_>) -> Option<Current> {
    let values = cx.client.setting_values().await?;
    Some(Current {
        enabled: values
            .get("rerank.enabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        url: values
            .get("rerank.url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

fn paths(cx: &Cx<'_>) -> Paths {
    let locations = &cx.env.locations;
    Paths::for_home(&locations.home, locations.xdg_config_home.as_deref())
}

impl<R: Runner> Rerank<R> {
    fn with_parts(
        runner: R,
        sources: Sources,
        base: &str,
        proxyd_candidates: Vec<PathBuf>,
    ) -> Self {
        Self {
            panel: Panel::default(),
            runner,
            sources,
            base: base.to_string(),
            proxyd_candidates,
        }
    }

    async fn succeeds(&self, program: &str, args: &[&str]) -> Option<String> {
        match self.runner.run(&Call::new(program, args)).await {
            Ok(out) if out.success => Some(out.stdout),
            _ => None,
        }
    }

    fn proxyd(&self) -> Option<PathBuf> {
        self.proxyd_candidates.iter().find(|p| p.is_file()).cloned()
    }

    async fn observe(&self, cx: &Cx<'_>) -> RerankInputs {
        let paths = paths(cx);
        let gpu = self
            .succeeds("nvidia-smi", &["-L"])
            .await
            .and_then(|out| out.lines().next().map(|l| l.trim().to_string()))
            .filter(|line| !line.is_empty());
        let systemd = self
            .succeeds("systemctl", &["--user", "show-environment"])
            .await
            .is_some();
        let socket_active = systemd
            && self
                .succeeds("systemctl", &["--user", "is-active", SOCKET_UNIT])
                .await
                .is_some();
        RerankInputs {
            gpu,
            systemd,
            proxyd: self.proxyd(),
            server_installed: paths.server().is_file(),
            model_installed: paths.model().is_file(),
            socket_active,
            current: current_settings(cx).await,
        }
    }

    async fn install_here(&self, cx: &mut Cx<'_>) -> Vec<Finding> {
        let paths = paths(cx);
        let Some(proxyd) = self.proxyd() else {
            return vec![Finding::failed("systemd-socket-proxyd is not installed.")];
        };
        let mut findings = Vec::new();
        for stage in 0..4 {
            let finding = match stage {
                0 => ensure_llama(&self.runner, &self.sources, &paths).await,
                1 => ensure_model(&self.sources, &paths).await,
                2 => write_units(&paths, &render_units(&paths, &proxyd)),
                _ => enable(&self.runner).await,
            };
            let passed = finding.passed();
            findings.push(finding);
            if !passed {
                return findings;
            }
        }
        findings.extend(self.apply(cx, &self.base).await);
        findings
    }

    async fn apply(&self, cx: &mut Cx<'_>, url: &str) -> Vec<Finding> {
        let checked = check_call(url).await;
        if !checked.passed() {
            return vec![checked];
        }
        let mut findings = vec![checked];
        findings.extend(ensure_proxy(cx).await);
        let sent = send_settings(cx.client, url).await;
        let sent_ok = all_passed(&sent);
        findings.extend(sent);
        if sent_ok && url == self.base {
            findings.push(Finding::note(
                "Other machines can use this reranker at this machine's address on port 11435.",
            ));
        }
        findings
    }

    async fn execute(&mut self, cx: &mut Cx<'_>) {
        let Some(action) = self.panel.selected().map(|c| c.action) else {
            return;
        };
        let typed = self.panel.input_value();
        let findings = match action {
            RerankAction::InstallHere => self.install_here(cx).await,
            RerankAction::OtherMachine => self.apply(cx, &typed).await,
            RerankAction::Leave => vec![Finding::note(
                "Reranking stays as it is. Searches keep the order search by meaning gives them.",
            )],
        };
        self.panel.record(findings);
    }
}

#[async_trait(?Send)]
impl<R: Runner> WizardStep for Rerank<R> {
    fn names(&self) -> &'static [&'static str] {
        &["rerank"]
    }

    fn label(&self) -> &'static str {
        "rerank"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        let inputs = self.observe(cx).await;
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
        self.panel.draw(frame, area, "search reranker");
    }

    fn hints(&self) -> Hints {
        self.panel.hints()
    }

    fn editing(&self) -> bool {
        self.panel.editing
    }
}
