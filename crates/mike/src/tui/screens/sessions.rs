use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::client::{ApiClient, SessionSummary};
use crate::tui::theme::{self, Tone, glyph};
use crate::tui::widgets::{ShortcutsSection, block, render_status_bar};

use super::Nav;

const PAGE_SIZE: usize = 50;

pub struct SessionsState {
    sessions: Vec<SessionSummary>,
    selected: usize,
    offset: usize,
    page_full: bool,
    error: Option<String>,
}

impl SessionsState {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            selected: 0,
            offset: 0,
            page_full: false,
            error: None,
        }
    }

    pub async fn refresh(&mut self, client: &ApiClient) {
        match client.list_sessions(self.offset, PAGE_SIZE).await {
            Some(mut sessions) => {
                self.page_full = sessions.len() == PAGE_SIZE;
                sessions.sort_by(|a, b| b.first_seen.cmp(&a.first_seen));
                self.selected = self.selected.min(sessions.len().saturating_sub(1));
                self.sessions = sessions;
                self.error = None;
            }
            None => {
                self.sessions.clear();
                self.error = Some("sessions unavailable — live store unreachable".to_string());
            }
        }
    }

    pub fn selected_session_id(&self) -> Option<String> {
        self.sessions.get(self.selected).map(|s| s.id.clone())
    }
}

impl Default for SessionsState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn draw(frame: &mut Frame, area: Rect, state: &mut SessionsState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let block = block("sessions", Tone::Uv, true);
    let inner = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);
    draw_body(frame, inner, state);

    let message = match &state.error {
        Some(e) => e.clone(),
        None => format!(
            "{} sessions · offset {}{}",
            state.sessions.len(),
            state.offset,
            if state.page_full { " · more →" } else { "" }
        ),
    };
    render_status_bar(
        frame,
        chunks[1],
        "SESSIONS",
        theme::UV,
        &message,
        &[
            ("↑↓/jk", "move"),
            ("⏎", "open requests"),
            ("PgDn/PgUp", "page"),
            ("r", "refresh"),
            ("Esc", "home"),
            ("q", "quit"),
        ],
    );
}

fn draw_body(frame: &mut Frame, area: Rect, state: &SessionsState) {
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

fn row_lines(state: &SessionsState) -> Vec<Line<'static>> {
    if state.sessions.is_empty() {
        return vec![Line::styled(
            "no sessions captured yet",
            Style::default().fg(theme::FG_3),
        )];
    }
    state
        .sessions
        .iter()
        .enumerate()
        .map(|(idx, s)| {
            let selected = idx == state.selected;
            let marker = if selected { glyph::SELECTION } else { ' ' };
            let (status_glyph, status_color, status_word) = if s.busy {
                (glyph::FILLED_CIRCLE, theme::ACID, "busy")
            } else {
                (glyph::OPEN_CIRCLE, theme::FG_3, "idle")
            };
            let id_style = if selected {
                Style::default().fg(theme::UV).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::FG_1)
            };
            let project = s.project.as_deref().unwrap_or("(no project)");
            let title = s.title.as_deref().unwrap_or("");
            let model = s.models.first().map(String::as_str).unwrap_or("—");
            Line::from(vec![
                Span::styled(format!("{marker} "), Style::default().fg(theme::RUST)),
                Span::styled(
                    format!("{status_glyph} "),
                    Style::default().fg(status_color),
                ),
                Span::styled(
                    format!("{status_word:<6}"),
                    Style::default().fg(status_color),
                ),
                Span::styled(format!("{:<38}", s.id), id_style),
                Span::styled(
                    format!("{:>4} reqs  ", s.request_count),
                    Style::default().fg(theme::FG_2),
                ),
                Span::styled(format!("{model:<20}"), Style::default().fg(theme::FG_2)),
                Span::styled(format!("{project:<24}"), Style::default().fg(theme::FG_2)),
                Span::styled(title.to_string(), Style::default().fg(theme::FG_3)),
            ])
        })
        .collect()
}

pub async fn handle_key(state: &mut SessionsState, key: KeyEvent, client: &ApiClient) -> Nav {
    match key.code {
        KeyCode::Char('q') => return Nav::Quit,
        KeyCode::Esc => return Nav::Home,
        KeyCode::Char('j') | KeyCode::Down if !state.sessions.is_empty() => {
            state.selected = (state.selected + 1).min(state.sessions.len() - 1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.selected = state.selected.saturating_sub(1);
        }
        KeyCode::Char('g') => state.selected = 0,
        KeyCode::Char('G') => state.selected = state.sessions.len().saturating_sub(1),
        KeyCode::Char('r') => state.refresh(client).await,
        KeyCode::PageDown if state.page_full => {
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
            if let Some(id) = state.selected_session_id() {
                return Nav::OpenSessionRequests(id);
            }
        }
        _ => {}
    }
    Nav::None
}

pub fn shortcuts() -> Vec<ShortcutsSection> {
    vec![
        crate::tui::widgets::NAV_RAIL_SHORTCUTS,
        ShortcutsSection {
            heading: "sessions",
            entries: &[
                ("↑↓/jk", "move"),
                ("⏎", "open the session's requests"),
                ("PgDn/PgUp", "next/prev page"),
                ("r", "refresh"),
                ("Esc", "home"),
                ("q", "quit"),
            ],
        },
    ]
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

    fn session(id: &str, first_seen: &str, busy: bool, request_count: usize) -> SessionSummary {
        SessionSummary {
            id: id.to_string(),
            first_seen: first_seen.to_string(),
            request_count,
            models: vec!["claude-sonnet-5".to_string()],
            project: Some("temet-proxy".to_string()),
            title: Some("fix the thing".to_string()),
            busy,
        }
    }

    #[test]
    fn renders_one_status_vocabulary_busy_and_idle_never_up_or_down() {
        let mut state = SessionsState::new();
        state.sessions = vec![
            session(
                "aaaaaaaa-0000-0000-0000-000000000001",
                "2026-08-16T09:00:00Z",
                true,
                3,
            ),
            session(
                "bbbbbbbb-0000-0000-0000-000000000002",
                "2026-08-15T09:00:00Z",
                false,
                1,
            ),
        ];
        let text = render_to_string(140, 20, |frame, area| draw(frame, area, &mut state));
        assert!(text.contains("busy"), "{text}");
        assert!(text.contains("idle"), "{text}");
        for banned in ["up", "down", "active", "UP", "DOWN", "ACTIVE"] {
            assert!(
                !text.contains(banned),
                "banned status word {banned:?} leaked onto the sessions screen: {text}"
            );
        }
    }

    #[test]
    fn empty_state_renders_without_panicking() {
        let mut state = SessionsState::new();
        let text = render_to_string(100, 20, |frame, area| draw(frame, area, &mut state));
        assert!(text.contains("no sessions captured yet"), "{text}");
    }

    #[test]
    fn j_k_g_shift_g_move_selection_within_bounds() {
        let mut state = SessionsState::new();
        state.sessions = (0..5)
            .map(|i| session(&format!("id-{i}"), "2026-08-16T09:00:00Z", false, 0))
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
    fn enter_opens_the_selected_sessions_requests() {
        let mut state = SessionsState::new();
        state.sessions = vec![session(
            "aaaaaaaa-0000-0000-0000-000000000001",
            "2026-08-16T09:00:00Z",
            false,
            0,
        )];
        let client = test_client();
        let nav = futures::executor::block_on(handle_key(&mut state, key(KeyCode::Enter), &client));
        assert_eq!(
            nav,
            Nav::OpenSessionRequests("aaaaaaaa-0000-0000-0000-000000000001".to_string())
        );
    }

    #[test]
    fn q_quits_and_esc_returns_home() {
        let mut state = SessionsState::new();
        let client = test_client();
        let nav =
            futures::executor::block_on(handle_key(&mut state, key(KeyCode::Char('q')), &client));
        assert_eq!(nav, Nav::Quit);
        let nav = futures::executor::block_on(handle_key(&mut state, key(KeyCode::Esc), &client));
        assert_eq!(nav, Nav::Home);
    }

    #[tokio::test]
    async fn refresh_sorts_sessions_by_first_seen_not_last_seen() -> anyhow::Result<()> {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/sessions"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!([
                    {
                        "id": "aaaaaaaa-0000-0000-0000-000000000001",
                        "first_seen": "2026-08-10T10:00:00Z",
                        "last_seen": "2026-08-16T09:00:00Z",
                        "request_count": 3,
                        "models": [],
                        "last_status": "Complete",
                        "project": "proj-a",
                        "busy": false
                    },
                    {
                        "id": "bbbbbbbb-0000-0000-0000-000000000002",
                        "first_seen": "2026-08-15T10:00:00Z",
                        "last_seen": "2026-08-15T10:05:00Z",
                        "request_count": 1,
                        "models": [],
                        "last_status": "Complete",
                        "project": "proj-b",
                        "busy": false
                    }
                ])),
            )
            .mount(&server)
            .await;

        let mut config = crate::config::ProxyConfig::for_tests_in_memory();
        config.inspect_listen_addr = server.address().to_string();
        let client = ApiClient::new(&config);

        let mut state = SessionsState::new();
        state.refresh(&client).await;

        assert_eq!(state.sessions.len(), 2);
        assert_eq!(
            state.sessions[0].id, "bbbbbbbb-0000-0000-0000-000000000002",
            "the more-recently-created session must render first even though \
             the OTHER session's `last_seen` is more recent: {:?}",
            state.sessions
        );
        assert_eq!(state.sessions[1].id, "aaaaaaaa-0000-0000-0000-000000000001");
        Ok(())
    }
}
