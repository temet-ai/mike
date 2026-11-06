use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::persist::names_an_environment_variable;
use crate::finding::{Finding, Kind, all_passed};
use crate::tui::theme::{self, Tone, glyph};
use crate::tui::widgets::{block, wrap_text};

#[derive(Debug, Clone)]
pub(super) struct Input {
    pub(super) label: &'static str,
    pub(super) placeholder: &'static str,
    pub(super) value: String,
    pub(super) secret: bool,
}

impl Input {
    pub(super) fn new(label: &'static str, placeholder: &'static str) -> Self {
        Self {
            label,
            placeholder,
            value: String::new(),
            secret: false,
        }
    }

    pub(super) fn secret(mut self) -> Self {
        self.secret = true;
        self
    }

    pub(super) fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    fn shown(&self, editing: bool) -> Span<'static> {
        let cursor = if editing { "▏" } else { "" };
        if self.value.is_empty() {
            let text = format!("{cursor}{}", self.placeholder);
            return Span::styled(text, Style::default().fg(theme::FG_DIM));
        }
        let body = if self.secret && !names_an_environment_variable(self.value.trim()) {
            "*".repeat(self.value.chars().count())
        } else {
            self.value.clone()
        };
        Span::styled(format!("{body}{cursor}"), Style::default().fg(theme::FG_1))
    }
}

#[derive(Debug, Clone)]
pub(super) struct Choice<A> {
    pub(super) action: A,
    pub(super) label: String,
    pub(super) why: String,
    pub(super) recommended: bool,
    pub(super) commands: Vec<String>,
    pub(super) root_commands: Vec<String>,
    pub(super) input: Option<Input>,
}

impl<A> Choice<A> {
    pub(super) fn new(action: A, label: impl Into<String>, why: impl Into<String>) -> Self {
        Self {
            action,
            label: label.into(),
            why: why.into(),
            recommended: false,
            commands: Vec::new(),
            root_commands: Vec::new(),
            input: None,
        }
    }

    pub(super) fn recommended(mut self) -> Self {
        self.recommended = true;
        self
    }

    pub(super) fn command(mut self, command: impl Into<String>) -> Self {
        self.commands.push(command.into());
        self
    }

    pub(super) fn root_command(mut self, command: impl Into<String>) -> Self {
        self.root_commands.push(command.into());
        self
    }

    pub(super) fn input(mut self, input: Input) -> Self {
        self.input = Some(input);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PanelEvent {
    Idle,
    Run,
    Next,
    Back,
    Skip,
    Recheck,
    FinishLater,
}

#[derive(Debug, Clone)]
pub(super) struct Panel<A> {
    pub(super) found: Vec<String>,
    pub(super) note: Option<String>,
    pub(super) choices: Vec<Choice<A>>,
    pub(super) cursor: usize,
    pub(super) result: Vec<Finding>,
    pub(super) done: bool,
    pub(super) editing: bool,
}

impl<A> Default for Panel<A> {
    fn default() -> Self {
        Self {
            found: Vec::new(),
            note: None,
            choices: Vec::new(),
            cursor: 0,
            result: Vec::new(),
            done: false,
            editing: false,
        }
    }
}

impl<A> Panel<A> {
    pub(super) fn new(found: Vec<String>, choices: Vec<Choice<A>>) -> Self {
        let cursor = choices.iter().position(|c| c.recommended).unwrap_or(0);
        Self {
            found,
            choices,
            cursor,
            ..Self::default()
        }
    }

    pub(super) fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    pub(super) fn selected(&self) -> Option<&Choice<A>> {
        self.choices.get(self.cursor)
    }

    pub(super) fn selected_mut(&mut self) -> Option<&mut Choice<A>> {
        self.choices.get_mut(self.cursor)
    }

    pub(super) fn input_value(&self) -> String {
        self.selected()
            .and_then(|c| c.input.as_ref())
            .map(|i| i.value.trim().to_string())
            .unwrap_or_default()
    }

    pub(super) fn record(&mut self, findings: Vec<Finding>) {
        self.done = all_passed(&findings) && !findings.is_empty();
        self.result = findings;
    }

    pub(super) fn show(&mut self, findings: Vec<Finding>) {
        self.done = false;
        self.result = findings;
    }

    fn move_by(&mut self, delta: isize) {
        let count = self.choices.len();
        if count == 0 {
            return;
        }
        let next = (self.cursor as isize + delta).rem_euclid(count as isize);
        self.cursor = next as usize;
        self.result.clear();
        self.done = false;
    }

    fn begin_edit(&mut self) {
        if self.selected().is_some_and(|c| c.input.is_some()) {
            self.editing = true;
        }
    }

    pub(super) fn handle(&mut self, key: KeyEvent) -> PanelEvent {
        if self.editing {
            self.edit(key);
            return PanelEvent::Idle;
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_by(-1),
            KeyCode::Home => self.move_by(-(self.cursor as isize)),
            KeyCode::End => self.move_by(self.choices.len() as isize - 1 - self.cursor as isize),
            KeyCode::Enter => return self.enter(),
            KeyCode::Char('e') => self.begin_edit(),
            KeyCode::Char('s') => return PanelEvent::Skip,
            KeyCode::Char('r') => return PanelEvent::Recheck,
            KeyCode::Char('f') => return PanelEvent::FinishLater,
            KeyCode::Left => return PanelEvent::Back,
            _ => {}
        }
        PanelEvent::Idle
    }

    fn enter(&mut self) -> PanelEvent {
        if self.done {
            return PanelEvent::Next;
        }
        let needs_text = self
            .selected()
            .and_then(|c| c.input.as_ref())
            .is_some_and(|i| i.value.trim().is_empty());
        if needs_text {
            self.editing = true;
            return PanelEvent::Idle;
        }
        PanelEvent::Run
    }

    fn edit(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => self.editing = false,
            _ => {
                if let Some(input) = self.selected_mut().and_then(|c| c.input.as_mut()) {
                    crate::tui::input::edit_text(&mut input.value, key);
                }
            }
        }
    }

    pub(super) fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.editing {
            return vec![("⏎", "accept"), ("Esc", "stop typing")];
        }
        if self.done {
            return vec![("⏎", "next"), ("r", "again"), ("←", "back"), ("f", "later")];
        }
        vec![("⏎", "run"), ("s", "skip"), ("←", "back"), ("f", "later")]
    }
}

fn heading(text: &str) -> Line<'static> {
    Line::styled(
        text.to_string(),
        Style::default().fg(theme::ICE).add_modifier(Modifier::BOLD),
    )
}

fn plain(text: &str, indent: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    wrap_text(text, width.saturating_sub(indent.chars().count()).max(10))
        .into_iter()
        .map(|line| Line::styled(format!("{indent}{line}"), style))
        .collect()
}

pub(in crate::tui::screens::setup) fn finding_lines(
    finding: &Finding,
    width: usize,
) -> Vec<Line<'static>> {
    let (mark, color) = match finding.kind {
        Kind::Ok => (glyph::FILLED_CIRCLE, theme::ACID),
        Kind::Note => (glyph::OPEN_DIAMOND, theme::ICE),
        Kind::Failed => (glyph::OPEN_CIRCLE, theme::RED),
    };
    wrap_text(&finding.text, width.saturating_sub(4).max(10))
        .into_iter()
        .enumerate()
        .map(|(n, text)| {
            let lead = if n == 0 {
                Span::styled(format!("  {mark} "), Style::default().fg(color))
            } else {
                Span::raw("    ")
            };
            Line::from(vec![
                lead,
                Span::styled(text, Style::default().fg(theme::FG_1)),
            ])
        })
        .collect()
}

impl<A> Panel<A> {
    fn choice_lines(&self, width: usize) -> Vec<Line<'static>> {
        let mut lines = vec![heading("Choices")];
        for (idx, choice) in self.choices.iter().enumerate() {
            let selected = idx == self.cursor;
            let marker = if selected { glyph::SELECTION } else { ' ' };
            let name = if choice.recommended {
                format!("{} (recommended)", choice.label)
            } else {
                choice.label.clone()
            };
            let style = if selected {
                Style::default().fg(theme::ICE).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::FG_1)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker} "), Style::default().fg(theme::ICE)),
                Span::styled(name, style),
            ]));
            lines.extend(plain(
                &choice.why,
                "    ",
                width,
                Style::default().fg(theme::FG_3),
            ));
            if let (true, Some(input)) = (selected, choice.input.as_ref()) {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("    {}: ", input.label),
                        Style::default().fg(theme::FG_2),
                    ),
                    input.shown(self.editing),
                ]));
            }
        }
        lines
    }

    fn command_lines(&self, width: usize) -> Vec<Line<'static>> {
        let Some(choice) = self.selected() else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        if !choice.commands.is_empty() {
            lines.push(Line::raw(""));
            lines.push(heading("Command"));
            for command in &choice.commands {
                lines.extend(plain(
                    command,
                    "  ",
                    width,
                    Style::default()
                        .fg(theme::FG_1)
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }
        if !choice.root_commands.is_empty() {
            lines.push(Line::raw(""));
            lines.push(heading("Run these yourself (mike never runs sudo)"));
            for command in &choice.root_commands {
                lines.extend(plain(
                    command,
                    "  ",
                    width,
                    Style::default().fg(theme::FG_1),
                ));
            }
        }
        lines
    }

    fn result_lines(&self, width: usize) -> Vec<Line<'static>> {
        if self.result.is_empty() {
            return Vec::new();
        }
        let mut lines = vec![Line::raw(""), heading("What was checked")];
        for finding in &self.result {
            lines.extend(finding_lines(finding, width));
        }
        lines
    }

    pub(super) fn lines(&self, width: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if !self.found.is_empty() {
            lines.push(heading("Found"));
            for item in &self.found {
                lines.extend(plain(item, "  ", width, Style::default().fg(theme::FG_1)));
            }
            lines.push(Line::raw(""));
        }
        if let Some(note) = &self.note {
            lines.extend(plain(note, "", width, Style::default().fg(theme::FG_2)));
            lines.push(Line::raw(""));
        }
        if !self.choices.is_empty() {
            lines.extend(self.choice_lines(width));
            lines.extend(self.command_lines(width));
        }
        lines.extend(self.result_lines(width));
        lines
    }

    pub(super) fn draw(&self, frame: &mut Frame, area: Rect, title: &str) {
        let b = block(title, Tone::Ice, true);
        let inner = b.inner(area);
        frame.render_widget(b, area);
        let lines = self.lines(inner.width as usize);
        let height = inner.height as usize;
        let offset = if lines.len() > height && (!self.result.is_empty() || self.cursor > 0) {
            lines.len().saturating_sub(height)
        } else {
            0
        };
        frame.render_widget(Paragraph::new(lines).scroll((offset as u16, 0)), inner);
    }
}
