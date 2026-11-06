use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::super::panel::{Choice, Panel, PanelEvent};
use super::super::step::{Cx, Hints, Outcome, WizardStep};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum Look {
    Yes,
    No,
}

pub(in super::super) struct Welcome {
    panel: Panel<Look>,
}

impl Welcome {
    pub(in super::super) fn new() -> Self {
        let choices = vec![
            Choice::new(
                Look::Yes,
                "Look at this machine",
                "Setup lists the tools it can use that are on this machine, with their versions, and offers only choices that work here.",
            )
            .recommended(),
            Choice::new(
                Look::No,
                "Do not look",
                "Each step then checks only the tools it needs when it runs.",
            ),
        ];
        let panel = Panel::new(Vec::new(), choices).with_note(
            "mike records the traffic between Claude Code and the Anthropic API so that you can inspect it. It captures every request and response in full, including your prompts, file contents, tool output and model answers, and stores them in a PostgreSQL database on this machine. Nothing is sent off this machine. Only programs on this machine can reach the proxy port; web pages are refused. `mike setup <step>` runs any one step again later.",
        );
        Self { panel }
    }
}

#[async_trait(?Send)]
impl WizardStep for Welcome {
    fn names(&self) -> &'static [&'static str] {
        &["welcome"]
    }

    fn label(&self) -> &'static str {
        "welcome"
    }

    async fn enter(&mut self, _cx: &mut Cx<'_>) {}

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        match self.panel.handle(key) {
            PanelEvent::Run | PanelEvent::Next => {
                cx.env.looked = self.panel.selected().is_none_or(|c| c.action == Look::Yes);
                Outcome::Next
            }
            PanelEvent::FinishLater => Outcome::FinishLater,
            _ => Outcome::Stay,
        }
    }

    fn draw(&self, frame: &mut Frame, area: Rect) {
        self.panel.draw(frame, area, "welcome");
    }

    fn hints(&self) -> Hints {
        vec![("↑↓", "pick"), ("⏎", "begin"), ("f", "later")]
    }
}
