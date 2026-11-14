use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::super::panel::{Choice, Panel, PanelEvent};
use super::super::step::{Cx, Hints, Outcome, WizardStep};
use super::proxy::ensure_proxy;
use crate::feature_presets::{Preset, apply_or_save, default_off_keys, plan};
use crate::feature_toggles::TOGGLE_META;
use crate::finding::Finding;

pub(in super::super) struct FeatureInputs {
    pub(in super::super) proxy_answers: bool,
    pub(in super::super) persistent: bool,
    pub(in super::super) switched_on: Vec<String>,
}

fn found_lines(inputs: &FeatureInputs) -> Vec<String> {
    let mut lines = vec![if inputs.proxy_answers {
        "A mike proxy answers on the inspect address".to_string()
    } else {
        "Nothing answers on the inspect address; the choice is saved and applied when the proxy next starts".to_string()
    }];
    lines.push(if inputs.persistent {
        "Switch settings are kept in the database".to_string()
    } else {
        "Captures are kept in memory only, and the proxy keeps no switch settings in that mode"
            .to_string()
    });
    if !inputs.switched_on.is_empty() {
        lines.push(format!(
            "Switched on now beyond the defaults: {}",
            inputs.switched_on.join(", ")
        ));
    }
    lines
}

fn turns_off(inputs: &FeatureInputs, preset: Preset) -> String {
    let wanted = plan(preset);
    let off: Vec<&str> = inputs
        .switched_on
        .iter()
        .filter(|key| {
            wanted
                .toggles
                .iter()
                .any(|(k, on)| k == &key.as_str() && !*on)
        })
        .map(String::as_str)
        .collect();
    if off.is_empty() {
        String::new()
    } else {
        format!(
            " It turns off these switches that are on now: {}.",
            off.join(", ")
        )
    }
}

pub(in super::super) fn plan_panel(inputs: &FeatureInputs) -> Panel<Preset> {
    let all = TOGGLE_META.len();
    let choices = vec![
        Choice::new(
            Preset::WatchOnly,
            "Watch only",
            format!(
                "mike records and shows the traffic and changes none of it. Every switch that rewrites a request or sends captured text to another service is off, and the rest keep their defaults.{}",
                turns_off(inputs, Preset::WatchOnly)
            ),
        )
        .command("mike features watch-only"),
        Choice::new(
            Preset::Recommended,
            "Recommended",
            format!(
                "Watch only, plus recall, lessons, reminders and code findings. Recall adds a note with lessons matched to the topic of the request. Lessons adds those of earlier sessions to the first turn of a session and to subagent turns. Reminders add a note built from the response analysis of the previous request. Code findings (\"Tell sessions what their edit added\") scans each file a session edits and tells the session what the edit added; it also switches on the change record and the code scan it needs. These notes are added to what Claude Code sends to Anthropic, and nothing leaves the machine for them.{}",
                turns_off(inputs, Preset::Recommended)
            ),
        )
        .recommended()
        .command("mike features recommended"),
        Choice::new(
            Preset::Everything,
            "Everything",
            format!(
                "All {all} switches on the settings page, including the ones that send captured text to OpenRouter (steerers, turn judge, session judge, lesson proposer, turn guard) and the ones that rewrite requests and responses. The ones that call a model do nothing until a provider is set up in the next steps."
            ),
        )
        .command("mike features everything"),
    ];
    Panel::new(found_lines(inputs), choices)
}

pub(in super::super) struct Features {
    panel: Panel<Preset>,
}

impl Features {
    pub(in super::super) fn new() -> Self {
        Self {
            panel: Panel::default(),
        }
    }

    async fn execute(&mut self, cx: &mut Cx<'_>) {
        let Some(preset) = self.panel.selected().map(|c| c.action) else {
            return;
        };
        let mut findings: Vec<Finding> = Vec::new();
        if !cx.config.persist_enabled {
            findings.push(Finding::note(
                "Captures are kept in memory only, so the proxy keeps no switch settings and everything stays at its default.",
            ));
            self.panel.record(findings);
            return;
        }
        findings.extend(ensure_proxy(cx).await);
        findings.extend(apply_or_save(cx.client, cx.env.config_file(), preset).await);
        self.panel.record(findings);
    }
}

#[async_trait(?Send)]
impl WizardStep for Features {
    fn names(&self) -> &'static [&'static str] {
        &["features"]
    }

    fn label(&self) -> &'static str {
        "features"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        let defaults_off = default_off_keys();
        let states = cx.client.feature_states().await;
        let inputs = FeatureInputs {
            proxy_answers: states.is_some(),
            persistent: cx.config.persist_enabled,
            switched_on: states
                .unwrap_or_default()
                .into_iter()
                .filter(|(key, on)| *on && defaults_off.contains(&key.as_str()))
                .map(|(key, _)| key)
                .collect(),
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
        self.panel.draw(frame, area, "features");
    }

    fn hints(&self) -> Hints {
        self.panel.hints()
    }
}
