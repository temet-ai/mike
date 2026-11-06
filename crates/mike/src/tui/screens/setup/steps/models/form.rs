use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::finding::{Finding, all_passed};
use crate::tui::screens::setup::panel::finding_lines;
use crate::tui::theme::{self, Tone, glyph};
use crate::tui::widgets::{block, wrap_text};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Cell {
    pub(super) options: Vec<String>,
    pub(super) index: usize,
    pub(super) custom: Option<String>,
    pub(super) typed: bool,
}

impl Cell {
    pub(super) fn pick(options: Vec<String>, selected: &str, typed: bool) -> Self {
        let index = options.iter().position(|o| o == selected);
        let custom = index.is_none().then(|| selected.to_string());
        Self {
            options,
            index: index.unwrap_or(0),
            custom: custom.filter(|c| !c.is_empty()),
            typed,
        }
    }

    pub(super) fn text(value: &str) -> Self {
        Self {
            options: Vec::new(),
            index: 0,
            custom: Some(value.to_string()),
            typed: true,
        }
    }

    pub(super) fn value(&self) -> String {
        match &self.custom {
            Some(custom) => custom.clone(),
            None => self.options.get(self.index).cloned().unwrap_or_default(),
        }
    }

    fn cycle(&mut self, delta: isize) -> bool {
        if self.options.len() < 2 {
            return false;
        }
        let count = self.options.len() as isize;
        self.index = (self.index as isize + delta).rem_euclid(count) as usize;
        self.custom = None;
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    Provider,
    Tier(super::jobs::Tier),
    Job(usize),
    DailySpend,
    MaxCalls,
    RatePerMinute,
    Submit,
}

#[derive(Debug, Clone)]
pub(super) struct Row {
    pub(super) role: Role,
    pub(super) label: String,
    pub(super) detail: String,
    pub(super) cells: Vec<Cell>,
    pub(super) focus: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FormEvent {
    Idle,
    Changed { row: usize, cell: usize },
    Submit,
    Skip,
    Back,
    FinishLater,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Form {
    pub(super) rows: Vec<Row>,
    pub(super) cursor: usize,
    pub(super) editing: bool,
    pub(super) note: Option<String>,
    pub(super) result: Vec<Finding>,
    pub(super) done: bool,
}

impl Form {
    pub(super) fn record(&mut self, findings: Vec<Finding>) {
        self.done = all_passed(&findings) && !findings.is_empty();
        self.result = findings;
    }

    pub(super) fn forget_result(&mut self) {
        self.done = false;
        self.result.clear();
    }

    fn move_row(&mut self, delta: isize) {
        let count = self.rows.len() as isize;
        if count == 0 {
            return;
        }
        self.cursor = (self.cursor as isize + delta).rem_euclid(count) as usize;
    }

    fn current_cell(&mut self) -> Option<&mut Cell> {
        let row = self.rows.get_mut(self.cursor)?;
        row.cells.get_mut(row.focus)
    }

    fn typing(&mut self, key: KeyEvent) -> FormEvent {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => self.editing = false,
            KeyCode::Backspace => {
                if let Some(cell) = self.current_cell() {
                    let mut text = cell.value();
                    text.pop();
                    cell.custom = Some(text);
                }
                self.forget_result();
            }
            KeyCode::Char(c) => {
                if let Some(cell) = self.current_cell() {
                    let mut text = cell.value();
                    text.push(c);
                    cell.custom = Some(text);
                }
                self.forget_result();
            }
            _ => {}
        }
        FormEvent::Idle
    }

    fn begin_typing(&mut self) {
        if self.current_cell().is_some_and(|c| c.typed) {
            self.editing = true;
        }
    }

    fn change(&mut self, delta: isize) -> FormEvent {
        let (row, cell) = (
            self.cursor,
            self.rows.get(self.cursor).map_or(0, |r| r.focus),
        );
        let changed = self.current_cell().is_some_and(|c| c.cycle(delta));
        if changed {
            FormEvent::Changed { row, cell }
        } else {
            FormEvent::Idle
        }
    }

    pub(super) fn handle(&mut self, key: KeyEvent) -> FormEvent {
        if self.editing {
            return self.typing(key);
        }
        let on_submit = self
            .rows
            .get(self.cursor)
            .is_some_and(|r| r.role == Role::Submit);
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.move_row(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_row(-1),
            KeyCode::Right | KeyCode::Char('l') => return self.change(1),
            KeyCode::Left | KeyCode::Char('h') => return self.change(-1),
            KeyCode::Tab => {
                if let Some(row) = self.rows.get_mut(self.cursor) {
                    row.focus = (row.focus + 1) % row.cells.len().max(1);
                }
            }
            KeyCode::Enter if on_submit => return FormEvent::Submit,
            KeyCode::Enter | KeyCode::Char('e') => self.begin_typing(),
            KeyCode::Char('s') => return FormEvent::Skip,
            KeyCode::Char('b') => return FormEvent::Back,
            KeyCode::Char('f') => return FormEvent::FinishLater,
            _ => {}
        }
        FormEvent::Idle
    }

    pub(super) fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.editing {
            return vec![("⏎", "accept"), ("Esc", "stop typing")];
        }
        if self.done {
            return vec![("⏎", "next"), ("b", "back"), ("f", "later")];
        }
        vec![
            ("←→", "change"),
            ("⏎", "save"),
            ("b", "back"),
            ("f", "later"),
        ]
    }

    fn row_lines(&self, index: usize, row: &Row) -> Vec<Line<'static>> {
        let selected = index == self.cursor;
        let marker = if selected { glyph::SELECTION } else { ' ' };
        if row.role == Role::Submit {
            let style = if selected {
                Style::default()
                    .fg(theme::ACID)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::FG_1)
            };
            return vec![Line::from(vec![
                Span::styled(format!("{marker} "), Style::default().fg(theme::ICE)),
                Span::styled(row.label.clone(), style),
            ])];
        }
        let mut spans = vec![
            Span::styled(format!("{marker} "), Style::default().fg(theme::ICE)),
            Span::styled(
                format!("{:<34}", row.label),
                Style::default().fg(theme::FG_1),
            ),
        ];
        for (n, cell) in row.cells.iter().enumerate() {
            let focused = selected && n == row.focus;
            let cursor = if focused && self.editing { "▏" } else { "" };
            let text = format!("{}{cursor}", cell.value());
            let shown = if cell.options.len() > 1 {
                format!("◂ {text} ▸ ")
            } else {
                format!("[{text}] ")
            };
            let style = if focused {
                Style::default().fg(theme::ICE).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::FG_2)
            };
            spans.push(Span::styled(shown, style));
        }
        let mut lines = vec![Line::from(spans)];
        if !row.detail.is_empty() {
            lines.push(Line::styled(
                format!("    {}", row.detail),
                Style::default().fg(theme::FG_3),
            ));
        }
        lines
    }

    pub(super) fn lines(&self, width: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if let Some(note) = &self.note {
            for text in wrap_text(note, width.max(10)) {
                lines.push(Line::styled(text, Style::default().fg(theme::FG_2)));
            }
            lines.push(Line::raw(""));
        }
        for (index, row) in self.rows.iter().enumerate() {
            lines.extend(self.row_lines(index, row));
        }
        if !self.result.is_empty() {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "What was checked",
                Style::default().fg(theme::ICE).add_modifier(Modifier::BOLD),
            ));
            for finding in &self.result {
                lines.extend(finding_lines(finding, width));
            }
        }
        lines
    }

    pub(super) fn draw(&self, frame: &mut Frame, area: Rect, title: &str) {
        let b = block(title, Tone::Ice, true);
        let inner = b.inner(area);
        frame.render_widget(b, area);
        let lines = self.lines(inner.width as usize);
        let height = inner.height as usize;
        let offset = if lines.len() > height && !self.result.is_empty() {
            lines.len() - height
        } else {
            0
        };
        frame.render_widget(Paragraph::new(lines).scroll((offset as u16, 0)), inner);
    }
}
