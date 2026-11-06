use super::*;
use crate::tui::client::{RequestStatusDto, TokenUsageDto};
use crate::tui::test_support::{render_to_lines, render_to_string};
use crossterm::event::KeyModifiers;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn test_client() -> ApiClient {
    ApiClient::new(&crate::config::ProxyConfig::for_tests_in_memory())
}

fn detail(human_prompt: &str) -> RequestDetail {
    RequestDetail {
        id: 42,
        session_id: "s-1".to_string(),
        request_number: Some(3),
        method: "POST".to_string(),
        path: "/v1/messages".to_string(),
        model: Some("claude-sonnet-5".to_string()),
        is_streaming: true,
        message_count: 3,
        human_prompt: human_prompt.to_string(),
        tool_names: vec!["Read".to_string()],
        project: Some("temet-proxy".to_string()),
        status: RequestStatusDto::Complete,
        request_body_size: 1024,
        response_body_size: 2048,
        token_usage: Some(TokenUsageDto {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: Some(10),
        }),
        stop_reason: Some("end_turn".to_string()),
        created_at: "2026-08-16T09:00:00Z".to_string(),
        completed_at: Some("2026-08-16T09:00:05Z".to_string()),
    }
}

#[test]
fn metadata_view_renders_full_human_prompt_verbatim() {
    let mut state = RequestDetailState::new("s-1".to_string(), 42);
    let line1 = "line one";
    let line2 = "line two";
    let line3 = "line three — nothing here is sliced";
    state.detail = Some(detail(&format!("{line1}\n{line2}\n{line3}")));
    let lines = render_to_lines(140, 30, |frame, area| draw(frame, area, &mut state));

    let stripped: Vec<&str> = lines.iter().map(|l| strip_chrome(l)).collect();
    let Some(row1) = stripped.iter().position(|l| *l == line1) else {
        panic!("the first prompt line must render on its own row, unmodified: {stripped:?}");
    };
    assert_eq!(
        stripped.get(row1 + 1).copied(),
        Some(line2),
        "the second prompt line must render on the row immediately after the \
         first, with nothing inserted between or appended to either: {stripped:?}"
    );
    assert_eq!(
        stripped.get(row1 + 2).copied(),
        Some(line3),
        "the third prompt line must render on the row immediately after the \
         second, with nothing inserted between or appended to either — the \
         full human_prompt text must render, not a truncated form: {stripped:?}"
    );
}

#[tokio::test]
async fn tab_cycles_metadata_request_body_response_body_and_back() {
    let mut state = RequestDetailState::new("s-1".to_string(), 42);
    let client = test_client();
    assert_eq!(state.view, DetailView::Metadata);
    handle_key(&mut state, key(KeyCode::Tab), &client).await;
    assert_eq!(state.view, DetailView::RequestBody);
    handle_key(&mut state, key(KeyCode::Tab), &client).await;
    assert_eq!(state.view, DetailView::ResponseBody);
    handle_key(&mut state, key(KeyCode::Tab), &client).await;
    assert_eq!(state.view, DetailView::Metadata);
    handle_key(&mut state, key(KeyCode::BackTab), &client).await;
    assert_eq!(state.view, DetailView::ResponseBody);
}

fn strip_chrome(row: &str) -> &str {
    row.trim_matches(|c: char| {
        c.is_whitespace() || matches!(c, '│' | '┃' | '┌' | '└' | '┐' | '┘' | '─')
    })
}

#[test]
fn response_body_view_renders_the_full_captured_text_verbatim_and_unmodified() {
    let mut state = RequestDetailState::new("s-1".to_string(), 42);
    state.view = DetailView::ResponseBody;
    let line1 = "event: message_start";
    let line2 = "full unsliced body content here";
    state.response_body = Some(BodyPayload {
        text: Some(format!("{line1}\n{line2}")),
        byte_size: 500_000,
        has_body: true,
    });
    let lines = render_to_lines(140, 20, |frame, area| draw(frame, area, &mut state));
    assert!(lines.join("\n").contains("never truncated"));

    let stripped: Vec<&str> = lines.iter().map(|l| strip_chrome(l)).collect();
    let Some(row1) = stripped.iter().position(|l| *l == line1) else {
        panic!("the first captured line must render on its own row, unmodified: {stripped:?}");
    };
    assert_eq!(
        stripped.get(row1 + 1).copied(),
        Some(line2),
        "the second captured line must render on the row immediately after the \
         first — nothing may be inserted between them or appended to either: \
         {stripped:?}"
    );
}

#[test]
fn body_pane_with_no_captured_body_says_so_not_a_blank_pane() {
    let mut state = RequestDetailState::new("s-1".to_string(), 42);
    state.view = DetailView::RequestBody;
    state.request_body = Some(BodyPayload {
        text: None,
        byte_size: 0,
        has_body: false,
    });
    let text = render_to_string(100, 20, |frame, area| draw(frame, area, &mut state));
    assert!(text.contains("no request body captured"), "{text}");
}

#[test]
fn esc_returns_to_the_originating_sessions_requests() {
    let mut state = RequestDetailState::new("s-42".to_string(), 42);
    let client = test_client();
    let nav = futures::executor::block_on(handle_key(&mut state, key(KeyCode::Esc), &client));
    assert_eq!(nav, Nav::OpenSessionRequests("s-42".to_string()));
}

#[test]
fn q_quits() {
    let mut state = RequestDetailState::new("s-1".to_string(), 42);
    let client = test_client();
    let nav = futures::executor::block_on(handle_key(&mut state, key(KeyCode::Char('q')), &client));
    assert_eq!(nav, Nav::Quit);
}

#[test]
fn scroll_keys_move_and_g_resets_to_top() {
    let mut state = RequestDetailState::new("s-1".to_string(), 42);
    let client = test_client();
    futures::executor::block_on(handle_key(&mut state, key(KeyCode::Char('j')), &client));
    futures::executor::block_on(handle_key(&mut state, key(KeyCode::PageDown), &client));
    assert!(state.body_scroll > 0);
    futures::executor::block_on(handle_key(&mut state, key(KeyCode::Char('g')), &client));
    assert_eq!(state.body_scroll, 0);
}
