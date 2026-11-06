use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::tui::theme::{self, Tone, glyph};
use crate::tui::widgets::{ShortcutsSection, render_status_bar};

use super::SetupState;

pub fn draw(frame: &mut Frame, area: Rect, state: &mut SetupState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);

    draw_indicator(frame, chunks[0], state);
    state.steps[state.current].draw(frame, chunks[1]);

    let last = state.current + 1 == state.steps.len();
    let (tone, mode) = if last {
        (Tone::Acid, "READY")
    } else {
        (Tone::Ice, "SETUP")
    };
    let step = &state.steps[state.current];
    let message = step.banner().unwrap_or_else(|| {
        let tail = match step.progress() {
            Some((n, of)) => format!("part {n}/{of}"),
            None => step.label().to_string(),
        };
        format!("step {}/{} · {tail}", state.current + 1, state.steps.len())
    });
    let keys = step.hints();
    render_status_bar(frame, chunks[2], mode, tone.color(), &message, &keys);
}

fn draw_indicator(frame: &mut Frame, area: Rect, state: &SetupState) {
    let mut spans = vec![
        Span::styled(
            "mike",
            Style::default().fg(theme::UV).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} setup  ", glyph::SEP),
            Style::default().fg(theme::FG_3),
        ),
    ];
    let last = state.steps.len() - 1;
    for (idx, step) in state.steps.iter().enumerate() {
        let (mark, color) = if idx < state.current {
            (glyph::FILLED_DIAMOND, theme::ACID)
        } else if idx == state.current && idx == last {
            (glyph::CURRENT_STEP, theme::ACID)
        } else if idx == state.current {
            (glyph::CURRENT_STEP, theme::ICE)
        } else {
            (glyph::OPEN_CIRCLE, theme::FG_DIM)
        };
        let text = if idx <= state.current {
            format!("{mark} {}·{}  ", idx + 1, step.label())
        } else {
            format!("{mark}  ")
        };
        spans.push(Span::styled(text, Style::default().fg(color)));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).wrap(Wrap { trim: false }),
        area,
    );
}

pub fn shortcuts(_state: &SetupState) -> Vec<ShortcutsSection> {
    let setup = ShortcutsSection {
        heading: "setup",
        entries: &[
            ("↑↓ / j k", "choose"),
            ("⏎", "run the command shown, or continue"),
            ("e", "type into the field of the chosen option"),
            ("r", "check again"),
            ("s", "skip the step"),
            ("←", "back a step"),
            ("f", "finish later"),
            ("Esc", "back to home"),
            ("q", "quit app"),
        ],
    };
    vec![crate::tui::widgets::NAV_RAIL_SHORTCUTS, setup]
}
