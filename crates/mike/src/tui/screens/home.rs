use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::config::ProxyConfig;
use crate::stack;
use crate::tui::client::{ApiClient, CapabilityRow, CapabilityState, MemoryUsagePayload};
use crate::tui::theme::{self, Tone, glyph};
use crate::tui::widgets::{ShortcutsSection, block, render_status_bar};

use super::Nav;
use super::start_stop::{describe_ensure, describe_stop, format_uptime};

pub struct HomeState {
    binds: Vec<(&'static str, String, bool)>,
    capabilities: Option<Vec<CapabilityRow>>,
    memory: Option<MemoryUsagePayload>,
    mcp_on_path: bool,
    running: bool,
    pid: Option<u32>,
    started_at: Option<std::time::SystemTime>,
    busy: bool,
    banner: Option<String>,
}

impl HomeState {
    pub fn new() -> Self {
        Self {
            binds: Vec::new(),
            capabilities: None,
            memory: None,
            mcp_on_path: false,
            running: false,
            pid: None,
            started_at: None,
            busy: false,
            banner: None,
        }
    }

    pub async fn refresh(&mut self, client: &ApiClient, config: &ProxyConfig) {
        if self.busy {
            return;
        }
        self.capabilities = client.capabilities().await.map(|p| p.capabilities);
        self.memory = client.memory_usage().await;
        self.mcp_on_path = crate::tui::on_path("mike-mcp");
        self.binds = client.bind_states().await;
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

    fn inspect_reachable(&self) -> bool {
        self.binds
            .iter()
            .any(|(name, _, up)| name.starts_with("inspect") && *up)
    }
}

impl Default for HomeState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn draw(frame: &mut Frame, area: Rect, state: &mut HomeState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let services_len = services_height(state);
    let body_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(services_len), Constraint::Min(0)])
        .split(chunks[0]);
    draw_services(frame, body_rows[0], state);

    let all_up = state.binds.iter().all(|(_, _, up)| *up);
    let steering_inert = state
        .capabilities
        .as_ref()
        .map(|caps| {
            caps.iter()
                .any(|c| c.id == "steering" && c.state == CapabilityState::Inert)
        })
        .unwrap_or(false);
    let up_count = state.binds.iter().filter(|(_, _, up)| *up).count();
    let message = if all_up {
        format!(
            "stack up · {up_count} binds healthy{}",
            if steering_inert {
                " · steering inert"
            } else {
                ""
            }
        )
    } else {
        format!(
            "stack down · {up_count}/{} binds healthy",
            state.binds.len()
        )
    };
    let mut keys = vec![if state.running {
        ("t", "stop")
    } else {
        ("t", "start")
    }];
    if state.running {
        keys.push(("r", "restart"));
    }
    keys.extend([("l", "logs"), ("s", "setup"), ("q", "quit")]);
    render_status_bar(frame, chunks[1], "ONLINE", theme::ACID, &message, &keys);
}

fn services_height(state: &HomeState) -> u16 {
    content_lines(state).len() as u16 + 1 + 1 + 2
}

fn draw_services(frame: &mut Frame, area: Rect, state: &HomeState) {
    let panel = block("services", Tone::Uv, true);
    let inner = panel.inner(area);
    frame.render_widget(panel, area);

    let lines = content_lines(state);
    let content_len = lines.len() as u16;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(content_len),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(Paragraph::new(lines), rows[0]);

    let status_line = if state.busy {
        Line::styled("working…", Style::default().fg(theme::FG_3))
    } else if state.running {
        let uptime = state
            .started_at
            .map(|t| format_uptime(t.elapsed().unwrap_or_default()))
            .unwrap_or_else(|| "?".to_string());
        let pid_text = state.pid.map(|p| format!(" · pid {p}")).unwrap_or_default();
        Line::from(vec![
            Span::styled(
                format!("{} stack running", glyph::FILLED_CIRCLE),
                Style::default()
                    .fg(theme::ACID)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{pid_text} · uptime {uptime}"),
                Style::default().fg(theme::FG_2),
            ),
        ])
    } else {
        Line::styled(
            format!("{} stack stopped", glyph::OPEN_CIRCLE),
            Style::default().fg(theme::FG_3),
        )
    };
    frame.render_widget(Paragraph::new(status_line), rows[1]);

    let banner = state.banner.as_deref().unwrap_or("");
    frame.render_widget(
        Paragraph::new(Line::styled(banner, Style::default().fg(theme::RUST))),
        rows[2],
    );
}

fn content_lines(state: &HomeState) -> Vec<Line<'static>> {
    let mut lines = vec![Line::raw("")];
    lines.push(section_label("BINDS"));
    lines.extend(bind_lines(state));
    lines.push(Line::raw(""));
    lines.push(section_label("SYSTEM"));
    lines.extend(system_lines(state));
    lines.push(Line::raw(""));
    lines.push(section_label("CAPABILITIES"));
    lines.extend(capability_lines(state));
    lines
}

fn bind_lines(state: &HomeState) -> Vec<Line<'static>> {
    if state.binds.is_empty() {
        return vec![Line::styled(
            "no binds configured",
            Style::default().fg(theme::FG_3),
        )];
    }
    state
        .binds
        .iter()
        .map(|(name, addr, up)| {
            let (glyph, color, word) = if *up {
                (glyph::FILLED_CIRCLE, theme::ACID, "up")
            } else {
                (glyph::OPEN_CIRCLE, theme::RED, "down")
            };
            Line::from(vec![
                Span::styled(format!("{glyph} "), Style::default().fg(color)),
                Span::styled(format!("{name:<16}"), Style::default().fg(theme::FG_1)),
                Span::styled(format!("{addr:<18}"), Style::default().fg(theme::FG_2)),
                Span::styled(word, Style::default().fg(color)),
            ])
        })
        .collect()
}

fn system_lines(state: &HomeState) -> Vec<Line<'static>> {
    let (store_text, store_color) = store_status(state);
    let (process_text, process_color) = process_status(state);
    let (mcp_text, mcp_color) = mcp_status(state);
    [
        ("store", store_text, store_color),
        ("process", process_text, process_color),
        ("mike-mcp", mcp_text, mcp_color),
    ]
    .into_iter()
    .map(|(label, text, color)| {
        Line::from(vec![
            Span::styled(format!("{label:<10}"), Style::default().fg(theme::FG_2)),
            Span::styled(text, Style::default().fg(color)),
        ])
    })
    .collect()
}

fn store_status(state: &HomeState) -> (String, ratatui::style::Color) {
    match &state.memory {
        None => (
            "usage unavailable (persistence off, or unreachable)".to_string(),
            theme::FG_3,
        ),
        Some(m) if m.max_store_bytes > 0 => {
            let pct = (m.entries_total_bytes as f64 / m.max_store_bytes as f64 * 100.0).round();
            let color = if m.over_byte_budget {
                theme::RUST
            } else {
                theme::ACID
            };
            (
                format!(
                    "{} / {} ({pct:.0}%) · {} entries",
                    format_bytes(m.entries_total_bytes),
                    format_bytes(m.max_store_bytes),
                    m.entry_count
                ),
                color,
            )
        }
        Some(m) => (
            format!(
                "{} · {} entries (no byte budget configured)",
                format_bytes(m.entries_total_bytes),
                m.entry_count
            ),
            theme::FG_1,
        ),
    }
}

fn process_status(state: &HomeState) -> (String, ratatui::style::Color) {
    match &state.memory {
        None => ("—".to_string(), theme::FG_3),
        Some(m) => match m.process_rss_bytes {
            Some(rss) => (format!("{} (peak)", format_bytes(rss)), theme::FG_1),
            None => ("unavailable on this platform".to_string(), theme::FG_3),
        },
    }
}

fn mcp_status(state: &HomeState) -> (String, ratatui::style::Color) {
    match (state.mcp_on_path, state.inspect_reachable()) {
        (true, true) => ("on PATH · inspect reachable".to_string(), theme::ACID),
        (true, false) => (
            "on PATH · inspect unreachable (calls will fail)".to_string(),
            theme::RUST,
        ),
        (false, _) => ("not found — run `make install`".to_string(), theme::RUST),
    }
}

fn capability_lines(state: &HomeState) -> Vec<Line<'static>> {
    match &state.capabilities {
        None => vec![Line::styled(
            "loading… (or the stack is down — try [t])",
            Style::default().fg(theme::FG_3),
        )],
        Some(caps) if caps.is_empty() => {
            vec![Line::styled("no data", Style::default().fg(theme::FG_3))]
        }
        Some(caps) => caps
            .iter()
            .map(|c| {
                let (glyph, color, label) = match c.state {
                    CapabilityState::Active => (glyph::FILLED_CIRCLE, theme::ACID, "up"),
                    CapabilityState::Degraded => (glyph::OPEN_CIRCLE, theme::ICE, "degraded"),
                    CapabilityState::Inert => (glyph::OPEN_CIRCLE, theme::RUST, "inert"),
                    CapabilityState::Off => (glyph::OPEN_CIRCLE, theme::FG_DIM, "off"),
                };
                Line::from(vec![
                    Span::styled(format!("{glyph} "), Style::default().fg(color)),
                    Span::styled(format!("{:<24}", c.label), Style::default().fg(theme::FG_1)),
                    Span::styled(label, Style::default().fg(color)),
                ])
            })
            .collect(),
    }
}

fn section_label(text: &'static str) -> Line<'static> {
    Line::styled(
        text,
        Style::default()
            .fg(theme::FG_3)
            .add_modifier(Modifier::BOLD),
    )
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1}GB", b / GB)
    } else if b >= MB {
        format!("{:.1}MB", b / MB)
    } else if b >= KB {
        format!("{:.1}KB", b / KB)
    } else {
        format!("{bytes}B")
    }
}

pub async fn handle_key(
    state: &mut HomeState,
    key: KeyEvent,
    client: &ApiClient,
    config: &ProxyConfig,
) -> Nav {
    match key.code {
        KeyCode::Char('q') => return Nav::Quit,
        KeyCode::Char('s') => return Nav::Setup,
        KeyCode::Char('l') => return Nav::StartStop,
        _ => {}
    }
    if state.busy {
        return Nav::None;
    }
    match key.code {
        KeyCode::Char('t') | KeyCode::Enter => {
            run_action(state, config, Action::Toggle).await;
        }
        KeyCode::Char('r') => {
            run_action(state, config, Action::Restart).await;
        }
        _ => return Nav::None,
    }
    state.refresh(client, config).await;
    Nav::None
}

enum Action {
    Toggle,
    Restart,
}

async fn run_action(state: &mut HomeState, config: &ProxyConfig, action: Action) {
    state.busy = true;
    state.banner = None;
    let result = match action {
        Action::Toggle if state.running => stack::stop_stack(config).await.map(describe_stop),
        Action::Toggle => stack::ensure_up(config).await.map(|o| describe_ensure(&o)),
        Action::Restart => stack::restart_stack(config)
            .await
            .map(|o| describe_ensure(&o)),
    };
    if let Err(e) = result {
        state.banner = Some(e.to_string());
    }
    state.busy = false;
}

pub fn shortcuts() -> Vec<ShortcutsSection> {
    vec![
        crate::tui::widgets::NAV_RAIL_SHORTCUTS,
        ShortcutsSection {
            heading: "services",
            entries: &[
                ("t", "toggle services"),
                ("r", "restart services"),
                ("l", "open the full service screen (logs)"),
                ("s", "setup"),
                ("q", "quit"),
            ],
        },
    ]
}

#[cfg(test)]
mod tests;
