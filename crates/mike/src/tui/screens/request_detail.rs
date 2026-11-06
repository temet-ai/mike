use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::tui::client::{ApiClient, BodyKind, BodyPayload, RequestDetail};
use crate::tui::theme::{self, Tone};
use crate::tui::widgets::{ShortcutsSection, block, render_status_bar};

use super::{Nav, request_status_glyph};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailView {
    Metadata,
    RequestBody,
    ResponseBody,
}

impl DetailView {
    fn next(self) -> Self {
        match self {
            Self::Metadata => Self::RequestBody,
            Self::RequestBody => Self::ResponseBody,
            Self::ResponseBody => Self::Metadata,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Metadata => Self::ResponseBody,
            Self::RequestBody => Self::Metadata,
            Self::ResponseBody => Self::RequestBody,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Metadata => "DETAIL",
            Self::RequestBody => "REQUEST BODY",
            Self::ResponseBody => "RESPONSE BODY",
        }
    }
}

pub struct RequestDetailState {
    session_id: String,
    request_id: u64,
    detail: Option<RequestDetail>,
    view: DetailView,
    request_body: Option<BodyPayload>,
    response_body: Option<BodyPayload>,
    body_scroll: u16,
    error: Option<String>,
}

impl RequestDetailState {
    pub fn new(session_id: String, request_id: u64) -> Self {
        Self {
            session_id,
            request_id,
            detail: None,
            view: DetailView::Metadata,
            request_body: None,
            response_body: None,
            body_scroll: 0,
            error: None,
        }
    }

    pub async fn refresh(&mut self, client: &ApiClient) {
        self.detail = client.request_detail(self.request_id).await;
        self.error = if self.detail.is_none() {
            Some("request detail unavailable — live store unreachable".to_string())
        } else {
            None
        };
    }

    async fn ensure_body_loaded(&mut self, client: &ApiClient) {
        match self.view {
            DetailView::Metadata => {}
            DetailView::RequestBody if self.request_body.is_none() => {
                self.request_body = client
                    .request_body(self.request_id, BodyKind::Request)
                    .await;
            }
            DetailView::ResponseBody if self.response_body.is_none() => {
                self.response_body = client
                    .request_body(self.request_id, BodyKind::Response)
                    .await;
            }
            _ => {}
        }
    }
}

pub fn draw(frame: &mut Frame, area: Rect, state: &mut RequestDetailState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let title = format!("request #{}", state.request_id);
    let b = block(&title, Tone::Uv, true);
    let inner = b.inner(chunks[0]);
    frame.render_widget(b, chunks[0]);

    let lines = match state.view {
        DetailView::Metadata => metadata_lines(state),
        DetailView::RequestBody => body_lines(&state.request_body, "request body"),
        DetailView::ResponseBody => body_lines(&state.response_body, "response body"),
    };
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((state.body_scroll, 0));
    frame.render_widget(paragraph, inner);

    let message = state.error.clone().unwrap_or_else(|| match &state.detail {
        Some(d) => format!("{} {}", d.method, d.path),
        None => "loading...".to_string(),
    });
    render_status_bar(
        frame,
        chunks[1],
        state.view.label(),
        theme::UV,
        &message,
        &[
            ("⇥/⇧⇥", "metadata/request/response"),
            ("↑↓/jk", "scroll"),
            ("PgDn/PgUp", "scroll a page"),
            ("Esc", "back"),
            ("q", "quit"),
        ],
    );
}

fn field_line(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<15}"), Style::default().fg(theme::FG_2)),
        Span::styled(value, Style::default().fg(theme::FG_1)),
    ])
}

fn metadata_lines(state: &RequestDetailState) -> Vec<Line<'static>> {
    let Some(d) = &state.detail else {
        return vec![Line::styled("loading...", Style::default().fg(theme::FG_3))];
    };
    let (status_glyph, status_color, status_word) = request_status_glyph(&d.status.to_string());
    let mut lines = vec![
        field_line(
            "id",
            match d.request_number {
                Some(n) => format!("#{} · turn {n}", d.id),
                None => format!("#{}", d.id),
            },
        ),
        field_line("session", d.session_id.clone()),
        field_line("method/path", format!("{} {}", d.method, d.path)),
        field_line("model", d.model.clone().unwrap_or_else(|| "—".to_string())),
        Line::from(vec![
            Span::styled(
                format!("{:<15}", "status"),
                Style::default().fg(theme::FG_2),
            ),
            Span::styled(
                format!("{status_glyph} "),
                Style::default().fg(status_color),
            ),
            Span::styled(status_word.to_string(), Style::default().fg(status_color)),
        ]),
        field_line(
            "streaming",
            if d.is_streaming { "yes" } else { "no" }.to_string(),
        ),
        field_line("messages", d.message_count.to_string()),
        field_line(
            "tools",
            if d.tool_names.is_empty() {
                "—".to_string()
            } else {
                d.tool_names.join(", ")
            },
        ),
        field_line(
            "project",
            d.project.clone().unwrap_or_else(|| "—".to_string()),
        ),
        field_line(
            "request bytes",
            format!("{} (full body one ⇥ away)", d.request_body_size),
        ),
        field_line(
            "response bytes",
            format!("{} (full body two ⇥ away)", d.response_body_size),
        ),
        field_line("created", d.created_at.clone()),
        field_line(
            "completed",
            d.completed_at.clone().unwrap_or_else(|| "—".to_string()),
        ),
    ];
    if let Some(usage) = &d.token_usage {
        lines.push(field_line(
            "tokens",
            format!(
                "in {} · out {} · cache-write {} · cache-read {}",
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_creation_input_tokens.unwrap_or(0),
                usage.cache_read_input_tokens.unwrap_or(0),
            ),
        ));
    }
    if let Some(reason) = &d.stop_reason {
        lines.push(field_line("stop reason", reason.clone()));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "HUMAN PROMPT (full text, scroll to read all)",
        Style::default()
            .fg(theme::FG_3)
            .add_modifier(Modifier::BOLD),
    ));
    if d.human_prompt.is_empty() {
        lines.push(Line::styled("(empty)", Style::default().fg(theme::FG_3)));
    } else {
        lines.extend(
            d.human_prompt
                .split('\n')
                .map(|l| Line::styled(l.to_string(), Style::default().fg(theme::FG_1))),
        );
    }
    lines
}

fn body_lines(payload: &Option<BodyPayload>, label: &str) -> Vec<Line<'static>> {
    match payload {
        None => vec![Line::styled("loading...", Style::default().fg(theme::FG_3))],
        Some(p) if !p.has_body => vec![Line::styled(
            format!("no {label} captured for this request"),
            Style::default().fg(theme::FG_3),
        )],
        Some(p) => {
            let mut lines = vec![
                Line::styled(
                    format!("{} bytes · full body, never truncated", p.byte_size),
                    Style::default()
                        .fg(theme::FG_3)
                        .add_modifier(Modifier::BOLD),
                ),
                Line::raw(""),
            ];
            match &p.text {
                Some(text) => lines.extend(text.split('\n').map(|l| Line::raw(l.to_string()))),
                None => lines.push(Line::styled(
                    "(binary or non-UTF8 body — not decodable as text)",
                    Style::default().fg(theme::FG_3),
                )),
            }
            lines
        }
    }
}

pub async fn handle_key(state: &mut RequestDetailState, key: KeyEvent, client: &ApiClient) -> Nav {
    match key.code {
        KeyCode::Char('q') => return Nav::Quit,
        KeyCode::Esc => return Nav::OpenSessionRequests(state.session_id.clone()),
        KeyCode::Tab => {
            state.view = state.view.next();
            state.body_scroll = 0;
            state.ensure_body_loaded(client).await;
        }
        KeyCode::BackTab => {
            state.view = state.view.prev();
            state.body_scroll = 0;
            state.ensure_body_loaded(client).await;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            state.body_scroll = state.body_scroll.saturating_add(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.body_scroll = state.body_scroll.saturating_sub(1);
        }
        KeyCode::PageDown => {
            state.body_scroll = state.body_scroll.saturating_add(20);
        }
        KeyCode::PageUp => {
            state.body_scroll = state.body_scroll.saturating_sub(20);
        }
        KeyCode::Char('g') => state.body_scroll = 0,
        KeyCode::Char('r') => state.refresh(client).await,
        _ => {}
    }
    Nav::None
}

pub fn shortcuts() -> Vec<ShortcutsSection> {
    vec![ShortcutsSection {
        heading: "request detail",
        entries: &[
            ("⇥/⇧⇥", "cycle metadata / request body / response body"),
            ("↑↓/jk", "scroll"),
            ("PgDn/PgUp", "scroll a page"),
            ("g", "scroll to top"),
            ("r", "refresh"),
            ("Esc", "back to requests"),
            ("q", "quit"),
        ],
    }]
}

#[cfg(test)]
mod tests;
