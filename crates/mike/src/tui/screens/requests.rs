use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::client::{ApiClient, RequestSummary};
use crate::tui::theme::{self, Tone, glyph};
use crate::tui::widgets::{ShortcutsSection, block, render_status_bar};

use super::{Nav, request_status_glyph};

const PAGE_SIZE: usize = 50;

pub struct RequestsState {
    session_id: String,
    requests: Vec<RequestSummary>,
    selected: usize,
    offset: usize,
    total: usize,
    error: Option<String>,
}

impl RequestsState {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            requests: Vec::new(),
            selected: 0,
            offset: 0,
            total: 0,
            error: None,
        }
    }

    pub async fn refresh(&mut self, client: &ApiClient) {
        match client
            .session_requests(&self.session_id, self.offset, PAGE_SIZE)
            .await
        {
            Some(mut page) => {
                page.requests
                    .sort_by(|a, b| b.created_at.cmp(&a.created_at));
                self.selected = self.selected.min(page.requests.len().saturating_sub(1));
                self.requests = page.requests;
                self.total = page.total_requests;
                self.error = None;
            }
            None => {
                self.requests.clear();
                self.error = Some("requests unavailable — live store unreachable".to_string());
            }
        }
    }

    pub fn selected_request_id(&self) -> Option<u64> {
        self.requests.get(self.selected).map(|r| r.id)
    }
}

pub fn draw(frame: &mut Frame, area: Rect, state: &mut RequestsState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let title = format!("requests · {}", state.session_id);
    let block = block(&title, Tone::Uv, true);
    let inner = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);
    draw_body(frame, inner, state);

    let message = match &state.error {
        Some(e) => e.clone(),
        None => format!(
            "{} of {} requests · offset {}",
            state.requests.len(),
            state.total,
            state.offset
        ),
    };
    render_status_bar(
        frame,
        chunks[1],
        "REQUESTS",
        theme::UV,
        &message,
        &[
            ("↑↓/jk", "move"),
            ("⏎", "open"),
            ("PgDn/PgUp", "page"),
            ("r", "refresh"),
            ("Esc", "sessions"),
            ("q", "quit"),
        ],
    );
}

fn draw_body(frame: &mut Frame, area: Rect, state: &RequestsState) {
    let lines = row_lines(state);
    let height = area.height as usize;
    let total = lines.len();
    let offset = if total <= height || state.selected < height / 2 {
        0
    } else {
        (state.selected - height / 2).min(total.saturating_sub(height))
    };
    let visible: Vec<Line> = lines.into_iter().skip(offset).collect();
    frame.render_widget(Paragraph::new(visible), area);
}

fn row_lines(state: &RequestsState) -> Vec<Line<'static>> {
    if state.requests.is_empty() {
        return vec![Line::styled(
            "no requests captured for this session",
            Style::default().fg(theme::FG_3),
        )];
    }
    state
        .requests
        .iter()
        .enumerate()
        .map(|(idx, r)| {
            let selected = idx == state.selected;
            let marker = if selected { glyph::SELECTION } else { ' ' };
            let (status_glyph, status_color, status_word) = request_status_glyph(&r.status);
            let id_style = if selected {
                Style::default().fg(theme::UV).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::FG_1)
            };
            let model = r.model.as_deref().unwrap_or("—");
            Line::from(vec![
                Span::styled(format!("{marker} "), Style::default().fg(theme::RUST)),
                Span::styled(
                    format!("{status_glyph} "),
                    Style::default().fg(status_color),
                ),
                Span::styled(
                    format!("{status_word:<10}"),
                    Style::default().fg(status_color),
                ),
                Span::styled(format!("#{:<8}", r.id), id_style),
                Span::styled(format!("{:<5}", r.method), Style::default().fg(theme::FG_3)),
                Span::styled(
                    format!("{:<10}", r.turn_origin),
                    Style::default().fg(theme::FG_2),
                ),
                Span::styled(format!("{model:<22}"), Style::default().fg(theme::FG_2)),
                Span::styled(
                    format!("{} tools", r.tool_names.len()),
                    Style::default().fg(theme::FG_3),
                ),
            ])
        })
        .collect()
}

pub async fn handle_key(state: &mut RequestsState, key: KeyEvent, client: &ApiClient) -> Nav {
    match key.code {
        KeyCode::Char('q') => return Nav::Quit,
        KeyCode::Esc => return Nav::Sessions,
        KeyCode::Char('j') | KeyCode::Down if !state.requests.is_empty() => {
            state.selected = (state.selected + 1).min(state.requests.len() - 1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.selected = state.selected.saturating_sub(1);
        }
        KeyCode::Char('g') => state.selected = 0,
        KeyCode::Char('G') => state.selected = state.requests.len().saturating_sub(1),
        KeyCode::Char('r') => state.refresh(client).await,
        KeyCode::PageDown if state.offset + state.requests.len() < state.total => {
            state.offset += PAGE_SIZE;
            state.selected = 0;
            state.refresh(client).await;
        }
        KeyCode::PageUp if state.offset > 0 => {
            state.offset = state.offset.saturating_sub(PAGE_SIZE);
            state.selected = 0;
            state.refresh(client).await;
        }
        KeyCode::Enter => {
            if let Some(request_id) = state.selected_request_id() {
                return Nav::OpenRequestDetail {
                    session_id: state.session_id.clone(),
                    request_id,
                };
            }
        }
        _ => {}
    }
    Nav::None
}

pub fn shortcuts() -> Vec<ShortcutsSection> {
    vec![ShortcutsSection {
        heading: "requests",
        entries: &[
            ("↑↓/jk", "move"),
            ("⏎", "open request detail"),
            ("PgDn/PgUp", "next/prev page"),
            ("r", "refresh"),
            ("Esc", "back to sessions"),
            ("q", "quit"),
        ],
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::test_support::render_to_string;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn test_client() -> ApiClient {
        ApiClient::new(&crate::config::ProxyConfig::for_tests_in_memory())
    }

    fn request(id: u64, status: &str, created_at: &str) -> RequestSummary {
        RequestSummary {
            id,
            method: "POST".to_string(),
            status: status.to_string(),
            model: Some("claude-sonnet-5".to_string()),
            created_at: created_at.to_string(),
            turn_origin: "primary".to_string(),
            tool_names: vec!["Read".to_string()],
        }
    }

    #[test]
    fn renders_the_four_status_words_lowercase_glyph_leading() {
        let mut state = RequestsState::new("s-1".to_string());
        state.requests = vec![
            request(1, "Pending", "2026-08-16T09:00:00Z"),
            request(2, "Streaming", "2026-08-16T09:01:00Z"),
            request(3, "Complete", "2026-08-16T09:02:00Z"),
            request(4, "Error: upstream 529", "2026-08-16T09:03:00Z"),
        ];
        state.total = 4;
        let text = render_to_string(140, 20, |frame, area| draw(frame, area, &mut state));
        for word in ["pending", "streaming", "complete", "error"] {
            assert!(text.contains(word), "missing {word:?} in: {text}");
        }
        for banned in ["PENDING", "STREAMING", "COMPLETE", "ERROR"] {
            assert!(!text.contains(banned), "uppercase leaked: {text}");
        }
    }

    #[test]
    fn empty_state_renders_without_panicking() {
        let mut state = RequestsState::new("s-1".to_string());
        let text = render_to_string(100, 20, |frame, area| draw(frame, area, &mut state));
        assert!(
            text.contains("no requests captured for this session"),
            "{text}"
        );
    }

    #[test]
    fn j_k_g_shift_g_move_selection_within_bounds() {
        let mut state = RequestsState::new("s-1".to_string());
        state.requests = (0..5)
            .map(|i| request(i, "Complete", "2026-08-16T09:00:00Z"))
            .collect();
        let client = test_client();

        futures::executor::block_on(handle_key(&mut state, key(KeyCode::Char('G')), &client));
        assert_eq!(state.selected, 4);
        futures::executor::block_on(handle_key(&mut state, key(KeyCode::Char('g')), &client));
        assert_eq!(state.selected, 0);
        futures::executor::block_on(handle_key(&mut state, key(KeyCode::Char('j')), &client));
        assert_eq!(state.selected, 1);
        futures::executor::block_on(handle_key(&mut state, key(KeyCode::Char('k')), &client));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn enter_opens_the_selected_requests_detail() {
        let mut state = RequestsState::new("s-1".to_string());
        state.requests = vec![request(7, "Complete", "2026-08-16T09:00:00Z")];
        let client = test_client();
        let nav = futures::executor::block_on(handle_key(&mut state, key(KeyCode::Enter), &client));
        assert_eq!(
            nav,
            Nav::OpenRequestDetail {
                session_id: "s-1".to_string(),
                request_id: 7,
            }
        );
    }

    #[test]
    fn q_quits_and_esc_returns_to_sessions() {
        let mut state = RequestsState::new("s-1".to_string());
        let client = test_client();
        let nav =
            futures::executor::block_on(handle_key(&mut state, key(KeyCode::Char('q')), &client));
        assert_eq!(nav, Nav::Quit);
        let nav = futures::executor::block_on(handle_key(&mut state, key(KeyCode::Esc), &client));
        assert_eq!(nav, Nav::Sessions);
    }

    #[tokio::test]
    async fn refresh_sorts_requests_by_created_at_desc() -> anyhow::Result<()> {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/sessions/s-1/requests"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "session_id": "s-1",
                    "requests": [
                        {
                            "id": 1,
                            "method": "POST",
                            "status": "Complete",
                            "model": "claude-sonnet-5",
                            "is_streaming": true,
                            "created_at": "2026-08-16T09:00:00Z",
                            "turn_origin": "primary",
                            "tool_names": []
                        },
                        {
                            "id": 2,
                            "method": "POST",
                            "status": "Complete",
                            "model": "claude-sonnet-5",
                            "is_streaming": true,
                            "created_at": "2026-08-16T09:05:00Z",
                            "turn_origin": "primary",
                            "tool_names": []
                        }
                    ],
                    "total_requests": 2,
                    "offset": 0,
                    "limit": 50
                })),
            )
            .mount(&server)
            .await;

        let mut config = crate::config::ProxyConfig::for_tests_in_memory();
        config.inspect_listen_addr = server.address().to_string();
        let client = ApiClient::new(&config);

        let mut state = RequestsState::new("s-1".to_string());
        state.refresh(&client).await;

        assert_eq!(state.requests.len(), 2);
        assert_eq!(
            state.requests[0].id, 2,
            "newest created_at must render first"
        );
        assert_eq!(state.requests[1].id, 1);
        assert_eq!(state.total, 2);
        Ok(())
    }
}
