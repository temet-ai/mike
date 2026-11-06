mod check;
mod plan;
#[cfg(test)]
mod tests;

use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::proxy::ensure_proxy;
use crate::db_provision::{Call, Runner, SystemRunner};
use crate::finding::{Finding, all_passed};
use crate::tui::screens::setup::facts::Tool;
use crate::tui::screens::setup::panel::{Panel, PanelEvent};
use crate::tui::screens::setup::step::{Cx, Hints, Outcome, WizardStep};
use check::{
    OLLAMA_BASE, OLLAMA_MODEL, fetch_one, has_model, ollama_models, pull_model, recall_row,
    send_settings, start_server,
};
use plan::{Current, EmbedAction, EmbedInputs, plan_panel};

pub(in crate::tui::screens::setup) struct Embeddings {
    panel: Panel<EmbedAction>,
    brew: bool,
    base: String,
}

async fn current_settings(cx: &Cx<'_>) -> Option<Current> {
    let values = cx.client.setting_values().await?;
    let text = |key: &str, fallback: &str| {
        values
            .get(key)
            .and_then(|v| v.as_str())
            .map_or_else(|| fallback.to_string(), str::to_string)
    };
    Some(Current {
        enabled: values
            .get("embedding.enabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        url: text("embedding.url", ""),
        model: text("embedding.model", ""),
    })
}

impl Embeddings {
    pub(in crate::tui::screens::setup) fn new() -> Self {
        Self::with_base(OLLAMA_BASE)
    }

    fn with_base(base: &str) -> Self {
        Self {
            panel: Panel::default(),
            brew: false,
            base: base.to_string(),
        }
    }

    async fn use_ollama(&self, cx: &mut Cx<'_>) -> Vec<Finding> {
        let mut findings = Vec::new();
        let mut models = ollama_models(&self.base).await;
        if models.is_none() {
            models = start_server(&self.base, self.brew).await;
        }
        let Some(models) = models else {
            return vec![Finding::failed(format!(
                "The Ollama server does not answer on {}. Start it with `ollama serve`, or with `brew services start ollama`, and run this step again.",
                self.base
            ))];
        };
        if has_model(&models, OLLAMA_MODEL) {
            findings.push(Finding::ok(format!("{OLLAMA_MODEL} is already pulled.")));
        } else {
            let pulled = pull_model().await;
            let pulled_ok = pulled.passed();
            findings.push(pulled);
            if !pulled_ok {
                return findings;
            }
        }
        let url = format!("{}/v1", self.base);
        findings.extend(self.apply(cx, &url, Some(OLLAMA_MODEL)).await);
        findings
    }

    async fn apply(&self, cx: &mut Cx<'_>, url: &str, model: Option<&str>) -> Vec<Finding> {
        let mut findings: Vec<Finding> = ensure_proxy(cx).await.into_iter().collect();
        let sent = send_settings(cx.client, url, model).await;
        let sent_ok = all_passed(&sent);
        findings.extend(sent);
        if !sent_ok {
            return findings;
        }
        let model = match model {
            Some(model) => model.to_string(),
            None => current_settings(cx)
                .await
                .map(|c| c.model)
                .unwrap_or_default(),
        };
        findings.push(fetch_one(url, &model).await);
        findings.push(recall_row(cx.client).await);
        findings
    }

    async fn install_with_brew(&mut self, cx: &mut Cx<'_>) {
        let call = Call::new("brew", &["install", "ollama"]);
        let installed = match SystemRunner.run(&call).await {
            Ok(out) if out.success => Finding::ok(format!("`{}` finished.", call.shown())),
            Ok(out) => Finding::failed(format!("`{}` failed: {}", call.shown(), out.detail())),
            Err(e) => Finding::failed(format!("`{}` could not run: {e}", call.shown())),
        };
        if !installed.passed() {
            self.panel.record(vec![installed]);
            return;
        }
        let mut findings = vec![installed];
        findings.push(match start_server(&self.base, true).await {
            Some(_) => Finding::ok("The Ollama server answers on 127.0.0.1:11434."),
            None => Finding::note(
                "The Ollama server did not answer yet; start it with `brew services start ollama`.",
            ),
        });
        self.enter(cx).await;
        self.panel.show(findings);
    }

    async fn execute(&mut self, cx: &mut Cx<'_>) {
        let Some(action) = self.panel.selected().map(|c| c.action) else {
            return;
        };
        let typed = self.panel.input_value();
        let findings = match action {
            EmbedAction::UseOllama => self.use_ollama(cx).await,
            EmbedAction::InstallWithBrew => {
                self.install_with_brew(cx).await;
                return;
            }
            EmbedAction::PrintInstall => {
                self.panel.show(vec![Finding::note(
                    "Nothing was run. After the install command has finished, run this step again.",
                )]);
                return;
            }
            EmbedAction::OtherServer => self.apply(cx, &typed, None).await,
            EmbedAction::Leave => vec![Finding::note(
                "Search by meaning stays as it is. Recall uses text matching while it is off.",
            )],
        };
        self.panel.record(findings);
    }
}

#[async_trait(?Send)]
impl WizardStep for Embeddings {
    fn names(&self) -> &'static [&'static str] {
        &["embeddings"]
    }

    fn label(&self) -> &'static str {
        "search"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        let path_var = cx.env.locations.path_var.clone();
        cx.env
            .facts
            .refresh(
                &SystemRunner,
                path_var.as_deref(),
                &[Tool::Ollama, Tool::Brew, Tool::Pacman, Tool::Apt, Tool::Dnf],
            )
            .await;
        self.brew = cx.env.facts.has(Tool::Brew);
        let inputs = EmbedInputs {
            base: self.base.clone(),
            ollama: cx.env.facts.get(Tool::Ollama).cloned(),
            models: ollama_models(&self.base).await,
            brew: self.brew,
            package_manager: cx.env.facts.package_manager(),
            current: current_settings(cx).await,
        };
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
        self.panel.draw(frame, area, "search by meaning");
    }

    fn hints(&self) -> Hints {
        self.panel.hints()
    }

    fn editing(&self) -> bool {
        self.panel.editing
    }
}
