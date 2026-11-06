use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::client::{ApiClient, CapabilityState, FeatureEntry, SettingEntry, SettingKind};
use crate::tui::theme::{self, Tone, glyph};
use crate::tui::widgets::{ShortcutsSection, block_with_hotkey, render_status_bar, wrap_text};

use super::Nav;

enum Row {
    Feature(FeatureEntry),
    Setting(SettingEntry),
    OpenRouterKeyEnvFlag(bool),
}

pub struct ConfigState {
    rows: Vec<Row>,
    capability_summary: String,
    selected: usize,
    error: Option<String>,
    pending_toggle: bool,
}

impl ConfigState {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            capability_summary: "loading capabilities…".to_string(),
            selected: 0,
            error: None,
            pending_toggle: false,
        }
    }

    pub async fn refresh(&mut self, client: &ApiClient) {
        let mut rows = Vec::new();
        if let Some(features) = client.features().await {
            rows.extend(features.features.into_iter().map(Row::Feature));
        }
        let mut openrouter_row = None;
        if let Some(settings) = client.settings().await {
            openrouter_row = Some(settings.steerer_openrouter_configured_via_env);
            rows.extend(settings.settings.into_iter().map(Row::Setting));
        }
        if let Some(configured) = openrouter_row {
            rows.push(Row::OpenRouterKeyEnvFlag(configured));
        }
        self.rows = rows;
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));

        self.capability_summary = match client.capabilities().await {
            Some(payload) => {
                let active = payload
                    .capabilities
                    .iter()
                    .filter(|c| c.state == CapabilityState::Active)
                    .count();
                let total = payload.capabilities.len();
                let inert: Vec<&str> = payload
                    .capabilities
                    .iter()
                    .filter(|c| c.state != CapabilityState::Active)
                    .map(|c| c.label.as_str())
                    .collect();
                if inert.is_empty() {
                    format!("{active}/{total} capabilities active")
                } else {
                    format!(
                        "{active}/{total} capabilities active {} {}",
                        glyph::SEP,
                        inert.join(", ")
                    )
                }
            }
            None => "capabilities unavailable — live store unreachable".to_string(),
        };
    }
}

impl Default for ConfigState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn draw(frame: &mut Frame, area: Rect, state: &mut ConfigState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(Line::styled(
            state.capability_summary.clone(),
            Style::default().fg(theme::ICE),
        )),
        chunks[0],
    );

    let block = block_with_hotkey("settings", 's', Tone::Ice, true);
    let inner = block.inner(chunks[1]);
    frame.render_widget(block, chunks[1]);
    draw_settings_body(frame, inner, state);

    let message = match &state.error {
        Some(e) => e.clone(),
        None => "live settings · postgres store".to_string(),
    };
    render_status_bar(
        frame,
        chunks[2],
        "CONFIG",
        theme::ICE,
        &message,
        &[
            ("↑↓/jk", "move"),
            ("⏎", "edit"),
            ("Esc", "home"),
            ("q", "quit"),
        ],
    );
}

fn draw_settings_body(frame: &mut Frame, area: Rect, state: &ConfigState) {
    let key_width = 32usize;
    let value_width = 12usize;
    let apply_width = (area.width as usize)
        .saturating_sub(key_width + value_width + 4)
        .max(10);

    let mut lines: Vec<Line> = Vec::new();
    let mut current_group: Option<&str> = None;
    let mut row_start_line = vec![0usize; state.rows.len()];

    for (idx, row) in state.rows.iter().enumerate() {
        let group = match row {
            Row::Feature(_) => "Feature toggles",
            Row::Setting(s) => s.group.as_str(),
            Row::OpenRouterKeyEnvFlag(_) => "Steerer",
        };
        if current_group != Some(group) {
            if current_group.is_some() {
                lines.push(Line::raw(""));
            }
            lines.push(Line::styled(
                group.to_uppercase(),
                Style::default().fg(theme::ICE).add_modifier(Modifier::BOLD),
            ));
            current_group = Some(group);
        }

        let selected = idx == state.selected;
        let (key, value_text, value_color, apply_text) = match row {
            Row::Feature(f) => (
                f.key.clone(),
                if f.enabled { "on" } else { "off" }.to_string(),
                if f.enabled { theme::ACID } else { theme::RUST },
                f.applies.label(),
            ),
            Row::Setting(s) => {
                let (text, color) = render_value(s);
                (s.key.clone(), text, color, s.applies.label())
            }
            Row::OpenRouterKeyEnvFlag(configured) => (
                "steerer.openrouter_api_key".to_string(),
                if *configured { "set" } else { "—" }.to_string(),
                if *configured {
                    theme::ACID
                } else {
                    theme::FG_DIM
                },
                "env-only, restart",
            ),
        };
        row_start_line[idx] = lines.len();
        let marker = if selected { glyph::SELECTION } else { ' ' };
        let key_style = if selected {
            Style::default().fg(theme::UV).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::FG_1)
        };
        let key_col_width = key_width.max(key.chars().count() + 2);
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} "), Style::default().fg(theme::RUST)),
            Span::styled(format!("{key:<key_col_width$}"), key_style),
            Span::styled(
                format!("{value_text:<width$}", width = value_width),
                Style::default().fg(value_color),
            ),
            Span::styled(apply_text, Style::default().fg(theme::FG_3)),
        ]));
        if let Row::Setting(s) = row {
            for wrapped in wrap_text(s.description.as_str(), apply_width) {
                lines.push(Line::styled(
                    format!("    {wrapped}"),
                    Style::default().fg(theme::FG_3),
                ));
            }
        }
    }

    let height = area.height as usize;
    let total = lines.len();
    let selected_line = row_start_line.get(state.selected).copied().unwrap_or(0);
    let offset = if total <= height || selected_line < height / 2 {
        0
    } else {
        (selected_line - height / 2).min(total - height)
    };
    let visible: Vec<Line> = lines.into_iter().skip(offset).collect();

    frame.render_widget(Paragraph::new(visible), area);
}

fn render_value(setting: &SettingEntry) -> (String, ratatui::style::Color) {
    match setting.kind {
        SettingKind::Bool => {
            let on = setting.value.as_bool().unwrap_or(false);
            (
                if on { "on" } else { "off" }.to_string(),
                if on { theme::ACID } else { theme::RUST },
            )
        }
        SettingKind::Text => {
            let text = setting.value.as_str().unwrap_or("");
            if text.is_empty() {
                ("—".to_string(), theme::FG_DIM)
            } else {
                (
                    format!(
                        "{} characters — read and edit it on /settings/workers",
                        text.chars().count()
                    ),
                    theme::FG_1,
                )
            }
        }
        _ => {
            let is_unset =
                setting.value.is_null() || setting.value.as_str().is_some_and(|s| s.is_empty());
            if is_unset {
                ("—".to_string(), theme::FG_DIM)
            } else {
                (
                    setting
                        .value
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| setting.value.to_string()),
                    theme::FG_1,
                )
            }
        }
    }
}

pub async fn handle_key(state: &mut ConfigState, key: KeyEvent, client: &ApiClient) -> Nav {
    if state.pending_toggle {
        return Nav::None;
    }
    match key.code {
        KeyCode::Char('q') => return Nav::Quit,
        KeyCode::Esc => return Nav::Home,
        KeyCode::Char('j') | KeyCode::Down if !state.rows.is_empty() => {
            state.selected = (state.selected + 1).min(state.rows.len() - 1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.selected = state.selected.saturating_sub(1);
        }
        KeyCode::Char('g') => state.selected = 0,
        KeyCode::Char('G') => state.selected = state.rows.len().saturating_sub(1),
        KeyCode::Enter => {
            state.pending_toggle = true;
            state.error = None;
            let outcome = match state.rows.get(state.selected) {
                Some(Row::Feature(f)) => Some(client.patch_feature(&f.key, !f.enabled).await),
                Some(Row::Setting(s)) if s.kind == SettingKind::Bool => {
                    let next = !s.value.as_bool().unwrap_or(false);
                    Some(
                        client
                            .patch_setting(&s.key, serde_json::Value::Bool(next))
                            .await,
                    )
                }
                _ => None,
            };
            if let Some(Err(e)) = outcome {
                state.error = Some(format!("write failed: {e}"));
            }
            state.pending_toggle = false;
            state.refresh(client).await;
        }
        _ => {}
    }
    Nav::None
}

pub fn shortcuts() -> Vec<ShortcutsSection> {
    vec![
        crate::tui::widgets::NAV_RAIL_SHORTCUTS,
        ShortcutsSection {
            heading: "config",
            entries: &[
                ("⏎", "edit selected setting"),
                ("esc", "back to control center"),
                ("q", "quit"),
            ],
        },
    ]
}

#[cfg(test)]
mod tests;
