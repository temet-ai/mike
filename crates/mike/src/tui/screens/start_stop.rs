use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, List as RList, ListItem, ListState, Paragraph, Row, Table};

use crate::config::ProxyConfig;
use crate::stack::{self, EnsureOutcome, StopOutcome};

use crate::tui::client::ApiClient;
use crate::tui::theme::{self, Tone, glyph};
use crate::tui::widgets::{ShortcutsSection, block_with_hotkey, render_status_bar, wrap_text};

use super::Nav;

const ACTIONS: [&str; 3] = ["Stop", "Start", "Restart"];

pub struct StartStopState {
    ports: Vec<(&'static str, String, bool)>,
    running: bool,
    pid: Option<u32>,
    started_at: Option<std::time::SystemTime>,
    log_lines: Vec<String>,
    actions: ListState,
    focus: usize,
    banner: Option<String>,
    busy: bool,
}

impl StartStopState {
    pub fn new() -> Self {
        let mut actions = ListState::default();
        actions.select(Some(0));
        Self {
            ports: Vec::new(),
            running: false,
            pid: None,
            started_at: None,
            log_lines: vec!["waiting for the stack…".to_string()],
            actions,
            focus: 1,
            banner: None,
            busy: false,
        }
    }

    pub async fn refresh(&mut self, client: &ApiClient, config: &ProxyConfig) {
        if self.busy {
            return;
        }
        self.ports = client.bind_states().await;
        self.running = stack::probe_selected(config).await;
        match stack::running_process(config) {
            Some((pid, started_at)) => {
                self.pid = Some(pid);
                self.started_at = Some(started_at);
            }
            None => {
                self.pid = None;
                self.started_at = None;
            }
        }
    }
}

impl Default for StartStopState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn draw(frame: &mut Frame, area: Rect, state: &mut StartStopState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let columns: Vec<Rect> = if chunks[0].width >= 100 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(54), Constraint::Percentage(46)])
            .split(chunks[0])
            .to_vec()
    } else {
        vec![chunks[0]]
    };
    let single_column = columns.len() == 1;

    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(4)])
        .split(columns[0]);
    draw_ports(frame, left_rows[0], state, state.focus == 0);
    draw_log(frame, left_rows[1], state, state.focus == 1);

    let right_area = if single_column {
        columns[0]
    } else {
        columns[1]
    };
    draw_lifecycle(frame, right_area, state, state.focus == 2);

    let mode = if state.running { "RUNNING" } else { "STOPPED" };
    let message = match (state.running, state.pid) {
        (true, Some(pid)) => {
            let uptime = state
                .started_at
                .map(|t| format_uptime(t.elapsed().unwrap_or_default()))
                .unwrap_or_else(|| "?".to_string());
            format!("stack up · pid {pid} · {uptime}")
        }
        (true, None) => "stack up · no mike-managed pidfile".to_string(),
        (false, _) => "stack down".to_string(),
    };
    render_status_bar(
        frame,
        chunks[1],
        mode,
        theme::ACID,
        &message,
        &[("x", "stop"), ("r", "restart"), ("l", "logs")],
    );
}

fn draw_ports(frame: &mut Frame, area: Rect, state: &StartStopState, focused: bool) {
    let block = block_with_hotkey("ports", 'p', Tone::Rust, focused);
    let rows: Vec<Row> = state
        .ports
        .iter()
        .map(|(name, addr, up)| {
            let (glyph, color) = if *up {
                (glyph::FILLED_CIRCLE, theme::ACID)
            } else {
                (glyph::OPEN_CIRCLE, theme::RED)
            };
            Row::new(vec![
                Cell::from(*name),
                Cell::from(addr.clone()),
                Cell::from(Span::styled(
                    format!("{glyph} {}", if *up { "up" } else { "down" }),
                    Style::default().fg(color),
                )),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Fill(1),
            Constraint::Length(10),
        ],
    )
    .block(block);
    frame.render_widget(table, area);
}

fn draw_log(frame: &mut Frame, area: Rect, state: &StartStopState, focused: bool) {
    let block = block_with_hotkey("log", 'l', Tone::Rust, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width.max(1) as usize;
    let lines: Vec<Line> = state
        .log_lines
        .iter()
        .flat_map(|l| wrap_text(l, width))
        .map(|l| Line::styled(l, Style::default().fg(theme::FG_2)))
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_lifecycle(frame: &mut Frame, area: Rect, state: &mut StartStopState, focused: bool) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if state.banner.is_some() {
            vec![
                Constraint::Length(2),
                Constraint::Length(3),
                Constraint::Min(3),
            ]
        } else {
            vec![Constraint::Length(2), Constraint::Min(3)]
        })
        .split(area);

    let (state_row, list_row, banner_row) = if state.banner.is_some() {
        (rows[0], rows[2], Some(rows[1]))
    } else {
        (rows[0], rows[1], None)
    };

    let state_line = if state.running {
        let since = state
            .started_at
            .map(|t| DateTime::<Local>::from(t).format("%H:%M").to_string())
            .unwrap_or_else(|| "?".to_string());
        let uptime = state
            .started_at
            .map(|t| format_uptime(t.elapsed().unwrap_or_default()))
            .unwrap_or_else(|| "?".to_string());
        let pid_text = state.pid.map(|p| format!(" · pid {p}")).unwrap_or_default();
        Line::from(vec![
            Span::styled(
                format!("{} running", glyph::FILLED_CIRCLE),
                Style::default()
                    .fg(theme::ACID)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {} since {since}{pid_text} · uptime {uptime}", glyph::SEP),
                Style::default().fg(theme::FG_2),
            ),
        ])
    } else {
        Line::from(Span::styled(
            format!("{} stopped", glyph::OPEN_CIRCLE),
            Style::default().fg(theme::FG_DIM),
        ))
    };
    frame.render_widget(Paragraph::new(state_line), state_row);

    if let Some(banner_area) = banner_row
        && let Some(banner) = &state.banner
    {
        let block = crate::tui::widgets::block("remediation", Tone::Rust, false);
        let inner = block.inner(banner_area);
        frame.render_widget(block, banner_area);
        frame.render_widget(
            Paragraph::new(Line::styled(
                banner.clone(),
                Style::default().fg(theme::RUST),
            )),
            inner,
        );
    }

    let block = block_with_hotkey("lifecycle", 's', Tone::Rust, focused);
    let items: Vec<ListItem> = ACTIONS
        .iter()
        .map(|label| {
            let dim = (*label == "Start" && state.running) || (*label == "Stop" && !state.running);
            let style = if state.busy {
                Style::default().fg(theme::FG_DIM)
            } else if dim {
                Style::default().fg(theme::FG_3)
            } else {
                Style::default().fg(theme::FG_1)
            };
            let suffix = if *label == "Start" && state.running {
                " (already running)"
            } else if *label == "Stop" && !state.running {
                " (not running)"
            } else {
                ""
            };
            ListItem::new(format!("{label}{suffix}")).style(style)
        })
        .collect();
    let highlight_symbol = format!("{} ", glyph::SELECTION);
    let list = RList::new(items)
        .block(block)
        .highlight_symbol(highlight_symbol.as_str())
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(theme::RUST),
        );
    frame.render_stateful_widget(list, list_row, &mut state.actions);
}

pub(crate) fn format_uptime(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h{m:02}m")
    } else if m > 0 {
        format!("{m}m{s:02}s")
    } else {
        format!("{s}s")
    }
}

pub async fn handle_key(
    state: &mut StartStopState,
    key: KeyEvent,
    client: &ApiClient,
    config: &ProxyConfig,
) -> Nav {
    match key.code {
        KeyCode::Char('q') => return Nav::Quit,
        KeyCode::Esc => return Nav::Home,
        KeyCode::Tab => {
            state.focus = (state.focus + 1) % 3;
            return Nav::None;
        }
        KeyCode::BackTab => {
            state.focus = (state.focus + 2) % 3;
            return Nav::None;
        }
        KeyCode::Char('p') => {
            state.focus = 0;
            return Nav::None;
        }
        KeyCode::Char('l') => {
            state.focus = 1;
            return Nav::None;
        }
        KeyCode::Char('s') => {
            state.focus = 2;
            return Nav::None;
        }
        _ => {}
    }
    if state.busy {
        return Nav::None;
    }
    match key.code {
        KeyCode::Char('x') => run_action(state, config, Action::Stop).await,
        KeyCode::Char('r') => run_action(state, config, Action::Restart).await,
        KeyCode::Char('j') | KeyCode::Down if state.focus == 2 => {
            let i = (state.actions.selected().unwrap_or(0) + 1) % ACTIONS.len();
            state.actions.select(Some(i));
        }
        KeyCode::Char('k') | KeyCode::Up if state.focus == 2 => {
            let i = (state.actions.selected().unwrap_or(0) + ACTIONS.len() - 1) % ACTIONS.len();
            state.actions.select(Some(i));
        }
        KeyCode::Char('g') if state.focus == 2 => state.actions.select(Some(0)),
        KeyCode::Char('G') if state.focus == 2 => state.actions.select(Some(ACTIONS.len() - 1)),
        KeyCode::Enter if state.focus == 2 => {
            let action = match state.actions.selected() {
                Some(0) => Action::Stop,
                Some(1) => Action::Start,
                Some(2) => Action::Restart,
                _ => return Nav::None,
            };
            run_action(state, config, action).await;
        }
        _ => {}
    }
    state.refresh(client, config).await;
    Nav::None
}

enum Action {
    Start,
    Stop,
    Restart,
}

async fn run_action(state: &mut StartStopState, config: &ProxyConfig, action: Action) {
    state.busy = true;
    state.banner = None;
    let result = match action {
        Action::Start => stack::ensure_up(config).await.map(|o| describe_ensure(&o)),
        Action::Stop => stack::stop_stack(config).await.map(describe_stop),
        Action::Restart => stack::restart_stack(config)
            .await
            .map(|o| describe_ensure(&o)),
    };
    match result {
        Ok(line) => state.log_lines.push(line),
        Err(e) => state.banner = Some(e.to_string()),
    }
    state.busy = false;
}

pub(crate) fn describe_ensure(outcome: &EnsureOutcome) -> String {
    match outcome {
        EnsureOutcome::AlreadyRunning => "already healthy — nothing to start".to_string(),
        EnsureOutcome::Spawned { pid, .. } => format!("started (pid {pid})"),
    }
}

pub(crate) fn describe_stop(outcome: StopOutcome) -> String {
    match outcome {
        StopOutcome::NotRunning => "not running".to_string(),
        StopOutcome::Stopped { pid } => format!("stopped (pid {pid})"),
    }
}

pub fn shortcuts() -> Vec<ShortcutsSection> {
    vec![
        ShortcutsSection {
            heading: "global",
            entries: &[
                ("↑↓/jk", "move selection"),
                ("⏎", "select"),
                ("⇥", "cycle panels"),
                ("⌃ /", "this modal"),
            ],
        },
        ShortcutsSection {
            heading: "service",
            entries: &[
                ("x", "stop the stack"),
                ("r", "restart"),
                ("l", "view logs"),
                ("q", "quit"),
            ],
        },
    ]
}

#[cfg(test)]
mod tests;
