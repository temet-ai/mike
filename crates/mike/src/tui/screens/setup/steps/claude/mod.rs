mod common;
mod mcp;
mod mods;
mod route;
mod shell;
#[cfg(test)]
mod tests;
mod trust;

use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::tui::screens::setup::step::{Cx, Hints, Outcome, WizardStep};

pub(in crate::tui::screens::setup) struct Claude {
    parts: Vec<Box<dyn WizardStep>>,
    current: usize,
}

impl Claude {
    pub(in crate::tui::screens::setup) fn new() -> Self {
        Self {
            parts: vec![
                Box::new(mcp::Mcp::new()),
                Box::new(route::RouteStep::new()),
                Box::new(shell::ShellStep::new()),
                Box::new(mods::ModsStep::new()),
                Box::new(trust::TrustStep::new()),
            ],
            current: 0,
        }
    }
}

#[async_trait(?Send)]
impl WizardStep for Claude {
    fn names(&self) -> &'static [&'static str] {
        &["claude"]
    }

    fn label(&self) -> &'static str {
        "claude code"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        self.current = 0;
        self.parts[0].enter(cx).await;
    }

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        let outcome = self.parts[self.current].key(key, cx).await;
        match outcome {
            Outcome::Next if self.current + 1 < self.parts.len() => {
                self.current += 1;
                self.parts[self.current].enter(cx).await;
                Outcome::Stay
            }
            Outcome::Back if self.current > 0 => {
                self.current -= 1;
                self.parts[self.current].enter(cx).await;
                Outcome::Stay
            }
            other => other,
        }
    }

    fn draw(&self, frame: &mut Frame, area: Rect) {
        self.parts[self.current].draw(frame, area);
    }

    fn hints(&self) -> Hints {
        self.parts[self.current].hints()
    }

    fn editing(&self) -> bool {
        self.parts[self.current].editing()
    }

    fn progress(&self) -> Option<(usize, usize)> {
        Some((self.current + 1, self.parts.len()))
    }
}
