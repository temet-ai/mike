use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;

use super::super::facts::Tool;
use super::super::panel::Panel;
use super::super::step::{Cx, Hints, Outcome, WizardStep};
use crate::db_provision::SystemRunner;

pub(in super::super) struct Machine {
    panel: Panel<()>,
}

impl Machine {
    pub(in super::super) fn new() -> Self {
        Self {
            panel: Panel::default(),
        }
    }

    async fn look(&mut self, cx: &mut Cx<'_>, again: bool) {
        if !cx.env.looked {
            self.panel = Panel::default().with_note(
                "You chose not to look at this machine. Each step checks only the tools it needs.",
            );
            return;
        }
        let path_var = cx.env.locations.path_var.clone();
        if again {
            cx.env
                .facts
                .refresh(&SystemRunner, path_var.as_deref(), &Tool::ALL)
                .await;
        } else {
            cx.env
                .facts
                .ensure(&SystemRunner, path_var.as_deref(), &Tool::ALL)
                .await;
        }
        let found = cx.env.facts.found_lines();
        let missing = cx.env.facts.not_found();
        let mut panel = Panel::new(found, Vec::new());
        if !missing.is_empty() {
            panel = panel.with_note(format!("Not found: {}.", missing.join(", ")));
        }
        self.panel = panel;
    }
}

#[async_trait(?Send)]
impl WizardStep for Machine {
    fn names(&self) -> &'static [&'static str] {
        &["machine"]
    }

    fn label(&self) -> &'static str {
        "machine"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        self.look(cx, false).await;
    }

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        match key.code {
            KeyCode::Enter => Outcome::Next,
            KeyCode::Left => Outcome::Back,
            KeyCode::Char('f') => Outcome::FinishLater,
            KeyCode::Char('r') => {
                self.look(cx, true).await;
                Outcome::Stay
            }
            _ => Outcome::Stay,
        }
    }

    fn draw(&self, frame: &mut Frame, area: Rect) {
        self.panel.draw(frame, area, "this machine");
    }

    fn hints(&self) -> Hints {
        vec![("⏎", "next"), ("r", "again"), ("←", "back"), ("f", "later")]
    }
}
