use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::common::{base_outcome, failure};
use crate::claude_code::{AUTO_TRUST_KEY, choose_trust, read_bool_setting};
use crate::tui::screens::setup::panel::{Choice, Panel, PanelEvent};
use crate::tui::screens::setup::step::{Cx, Hints, Outcome, WizardStep};

pub(super) fn plan_panel(stored: Option<bool>) -> Panel<bool> {
    let found = vec![match stored {
        Some(true) => "Saved choice: `mike claude` marks the folder as trusted".to_string(),
        Some(false) => "Saved choice: folder trust is left to Claude Code".to_string(),
        None => "No choice is saved yet".to_string(),
    }];
    let choices = vec![
        Choice::new(
            false,
            "Leave folder trust to Claude Code",
            "Claude Code asks once per folder whether you trust it, and `mike claude` does not touch that answer.",
        )
        .recommended()
        .command("mike claude-code trust off"),
        Choice::new(
            true,
            "Let `mike claude` mark the folder as trusted",
            "`mike claude` writes hasTrustDialogAccepted for the folder it starts in into ~/.claude.json, so the dialog never appears. Choose this only if you start Claude Code in folders you trust without reading them first.",
        )
        .command("mike claude-code trust on"),
    ];
    let mut panel = Panel::new(found, choices);
    if let Some(stored) = stored {
        panel.cursor = usize::from(stored);
    }
    panel
}

pub(super) struct TrustStep {
    panel: Panel<bool>,
}

impl TrustStep {
    pub(super) fn new() -> Self {
        Self {
            panel: Panel::default(),
        }
    }

    fn execute(&mut self, cx: &Cx<'_>) {
        let Some(trust) = self.panel.selected().map(|c| c.action) else {
            return;
        };
        let findings = match choose_trust(&cx.env.locations, trust) {
            Ok(findings) => findings,
            Err(e) => failure(e),
        };
        self.panel.record(findings);
    }
}

#[async_trait(?Send)]
impl WizardStep for TrustStep {
    fn names(&self) -> &'static [&'static str] {
        &["claude"]
    }

    fn label(&self) -> &'static str {
        "claude code"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        let stored = read_bool_setting(cx.env.config_file(), AUTO_TRUST_KEY)
            .ok()
            .flatten();
        self.panel = plan_panel(stored);
    }

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        let event = self.panel.handle(key);
        if let Some(outcome) = base_outcome(event) {
            return outcome;
        }
        if event == PanelEvent::Run {
            self.execute(cx);
        } else {
            self.enter(cx).await;
        }
        Outcome::Stay
    }

    fn draw(&self, frame: &mut Frame, area: Rect) {
        self.panel.draw(frame, area, "claude code · folder trust");
    }

    fn hints(&self) -> Hints {
        self.panel.hints()
    }
}
