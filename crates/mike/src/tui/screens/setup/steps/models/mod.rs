mod build;
mod check;
mod form;
mod intro;
mod jobs;
mod source;
#[cfg(test)]
mod tests;

use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::proxy::ensure_proxy;
use crate::finding::{Finding, all_passed};
use crate::service::observe_for_setup;
use crate::tui::screens::setup::panel::{Panel, PanelEvent};
use crate::tui::screens::setup::persist::names_an_environment_variable;
use crate::tui::screens::setup::step::{Cx, Hints, Outcome, WizardStep};
use build::{Mode, build_form, collect, refresh_after_change};
use form::{Form, FormEvent};
use intro::{IntroAction, idle_panel, plan_panel};
use source::Source;

const API_DOWN: &str = "The proxy is not running, so the models cannot be set now. Install the background service in the service step, or start it with `mike claude`, and run `mike setup models` again.";
const NO_JOBS: &str = "The features you chose run no background model job, so there is nothing to set here. `mike setup models` opens this step again after you switch on a feature that calls a model.";

enum Phase {
    Intro,
    Edit,
}

pub(in crate::tui::screens::setup) struct Models {
    phase: Phase,
    intro: Panel<IntroAction>,
    form: Form,
    source: Option<Source>,
    mode: Option<Mode>,
}

impl Models {
    pub(in crate::tui::screens::setup) fn new() -> Self {
        Self {
            phase: Phase::Intro,
            intro: Panel::default(),
            form: Form::default(),
            source: None,
            mode: None,
        }
    }

    async fn gather(cx: &mut Cx<'_>) -> Result<Source, &'static str> {
        ensure_proxy(cx).await;
        let Some(switches) = cx.client.feature_states().await else {
            return Err(API_DOWN);
        };
        let (providers, values) =
            tokio::join!(cx.client.llm_providers(), cx.client.setting_values());
        let (Some(providers), Some(values)) = (providers, values) else {
            return Err(API_DOWN);
        };
        let service_installed = observe_for_setup().await.is_some_and(|o| o.installed);
        Ok(Source::new(
            providers,
            values,
            &switches,
            cx.config
                .sections
                .get::<crate::lessons_settings::LessonRelationsConfig>()
                .enabled,
            service_installed,
        ))
    }

    async fn load(&mut self, cx: &mut Cx<'_>) {
        self.phase = Phase::Intro;
        match Self::gather(cx).await {
            Err(note) => {
                self.intro = idle_panel(note);
                self.source = None;
            }
            Ok(source) if source.running.is_empty() => {
                self.intro = idle_panel(NO_JOBS);
                self.source = Some(source);
            }
            Ok(source) => {
                let default = if std::env::var_os("OPENROUTER_API_KEY").is_some() {
                    "OPENROUTER_API_KEY"
                } else {
                    ""
                };
                self.intro = plan_panel(&source, default);
                self.source = Some(source);
            }
        }
    }

    fn open_editor(&mut self, mode: Mode) {
        if let Some(source) = &self.source {
            self.form = build_form(&mode, source);
            self.mode = Some(mode);
            self.phase = Phase::Edit;
        }
    }

    async fn run_intro(&mut self, cx: &mut Cx<'_>) -> Outcome {
        let Some(action) = self.intro.selected().map(|c| c.action.clone()) else {
            return Outcome::Next;
        };
        match action {
            IntroAction::Provider(id) => self.open_editor(Mode::One(id)),
            IntroAction::PerJob => self.open_editor(Mode::PerJob),
            IntroAction::SetUpOpenRouter => self.set_up_openrouter(cx).await,
            IntroAction::Leave => self.intro.record(vec![Finding::note(
                "Nothing was changed. `mike setup models` opens this step again.",
            )]),
        }
        Outcome::Stay
    }

    async fn set_up_openrouter(&mut self, cx: &mut Cx<'_>) {
        let typed = self.intro.input_value().trim().to_string();
        let by_name = names_an_environment_variable(&typed);
        let saved = if by_name {
            crate::setup::provider::apply_reference(cx.config, &typed).await
        } else {
            crate::setup::provider::apply(cx.config, &typed).await
        };
        if let Err(e) = saved {
            self.intro.record(vec![Finding::failed(e.to_string())]);
            return;
        }
        let mut findings = vec![Finding::ok(if by_name {
            format!(
                "Saved the variable name {typed} in {}; no key was written to disk.",
                cx.env.config_file().display()
            )
        } else {
            format!(
                "Saved the key in {}, which only you can read.",
                cx.env.config_file().display()
            )
        })];
        let key = if by_name {
            std::env::var(&typed).ok()
        } else {
            Some(typed.clone())
        };
        findings.push(match key.filter(|k| !k.is_empty()) {
            None => Finding::note(format!(
                "{typed} is empty in this terminal, so the key could not be checked now."
            )),
            Some(key) => match cx.client.put_openrouter_key(&key).await {
                Ok(label) => Finding::ok(format!(
                    "OpenRouter accepted the key {label}, and the running proxy uses it now."
                )),
                Err(why) => {
                    Finding::failed(format!("The running proxy could not use the key: {why}"))
                }
            },
        });
        if by_name && self.source.as_ref().is_some_and(|s| s.service_installed) {
            findings.push(Finding::note("The background service starts without your shell's variables, so after a restart it only finds the key if its own environment sets that variable. Paste the key instead if it does not."));
        }
        self.load(cx).await;
        self.intro.show(findings);
    }

    async fn submit(&mut self, cx: &mut Cx<'_>) {
        let (Some(source), Some(mode)) = (&self.source, &self.mode) else {
            return;
        };
        let settings = match collect(&self.form, source, mode) {
            Ok(settings) => settings,
            Err(why) => {
                self.form.record(vec![Finding::failed(why)]);
                return;
            }
        };
        let mut findings: Vec<Finding> = ensure_proxy(cx).await.into_iter().collect();
        let sent = check::send_settings(cx.client, &settings).await;
        let sent_ok = all_passed(&sent);
        findings.extend(sent);
        if sent_ok {
            findings.push(check::read_back(cx.client, &settings).await);
            findings.extend(check::provider_checks(cx.client, &settings, None).await);
            findings.extend(check::health_rows(cx.client).await);
        }
        self.form.record(findings);
    }

    async fn key_edit(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        match self.form.handle(key) {
            FormEvent::Submit if self.form.done => Outcome::Next,
            FormEvent::Submit => {
                self.submit(cx).await;
                Outcome::Stay
            }
            FormEvent::Changed { row, cell } => {
                self.form.forget_result();
                if let Some(source) = &self.source {
                    refresh_after_change(&mut self.form, source, row, cell);
                }
                Outcome::Stay
            }
            FormEvent::Skip => Outcome::Next,
            FormEvent::Back => {
                self.phase = Phase::Intro;
                Outcome::Stay
            }
            FormEvent::FinishLater => Outcome::FinishLater,
            FormEvent::Idle => Outcome::Stay,
        }
    }

    async fn key_intro(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        match self.intro.handle(key) {
            PanelEvent::Run => self.run_intro(cx).await,
            PanelEvent::Next | PanelEvent::Skip => Outcome::Next,
            PanelEvent::Back => Outcome::Back,
            PanelEvent::Recheck => {
                self.load(cx).await;
                Outcome::Stay
            }
            PanelEvent::FinishLater => Outcome::FinishLater,
            PanelEvent::Idle => Outcome::Stay,
        }
    }
}

#[async_trait(?Send)]
impl WizardStep for Models {
    fn names(&self) -> &'static [&'static str] {
        &["models"]
    }

    fn label(&self) -> &'static str {
        "models"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        self.load(cx).await;
    }

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        match self.phase {
            Phase::Intro => self.key_intro(key, cx).await,
            Phase::Edit => self.key_edit(key, cx).await,
        }
    }

    fn draw(&self, frame: &mut Frame, area: Rect) {
        match self.phase {
            Phase::Intro => self.intro.draw(frame, area, "background models"),
            Phase::Edit => self.form.draw(frame, area, "background models"),
        }
    }

    fn hints(&self) -> Hints {
        match self.phase {
            Phase::Intro if self.intro.choices.is_empty() => {
                vec![("⏎", "next"), ("r", "again"), ("←", "back"), ("f", "later")]
            }
            Phase::Intro => self.intro.hints(),
            Phase::Edit => self.form.hints(),
        }
    }

    fn editing(&self) -> bool {
        match self.phase {
            Phase::Intro => self.intro.editing,
            Phase::Edit => self.form.editing,
        }
    }
}
