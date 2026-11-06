use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;

use crossterm::event::KeyCode;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{Ready, Wait};
use crate::config::ProxyConfig;
use crate::doctor::{Check, Report, SetupStep};
use crate::tui::client::ApiClient;
use crate::tui::screens::Nav;
use crate::tui::screens::setup::harness::{Harness, key, render_step};
use crate::tui::screens::setup::step::{Outcome, WizardStep};

static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn record_open(url: &str) {
    if let Ok(mut opened) = OPENED.lock() {
        opened.push(url.to_string());
    }
}

fn opened() -> Vec<String> {
    OPENED.lock().map(|o| o.clone()).unwrap_or_default()
}

fn report(_: &ProxyConfig) -> Pin<Box<dyn Future<Output = Report> + '_>> {
    Box::pin(async {
        Report::new(vec![
            Check::ok(
                "database_url",
                "database url",
                "postgres://mike:***@127.0.0.1:6666/mike",
            ),
            Check::warn(
                "claude_route",
                "claude route",
                "no route is chosen",
                Some(SetupStep::Claude),
            ),
            Check::fail(
                "mcp_binary",
                "mike-mcp",
                "`mike-mcp` is neither on PATH nor beside `mike`",
                Some(SetupStep::Claude),
            ),
        ])
    })
}

fn at(poll: Duration) -> Ready {
    Ready::with_parts(report, record_open, poll)
}

fn request_row(id: u64) -> serde_json::Value {
    json!({
        "id": id, "session_id": "abc-123", "method": "POST", "path": "/v1/messages",
        "model": "claude-sonnet-5-5", "status": "complete", "created_at": "2026-10-16T21:30:00Z",
        "project": "demo", "is_streaming": true, "message_count": 3, "human_prompt": "hi"
    })
}

#[tokio::test]
async fn the_step_shows_the_doctor_checks_the_launch_command_and_that_it_is_waiting()
-> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let mut step = at(Duration::ZERO);
    step.enter(&mut harness.cx()).await;
    let text = render_step(&step);
    assert!(
        text.contains("Start Claude Code in any folder with `mike claude`."),
        "{text}"
    );
    assert!(
        text.contains("Waiting for the first request from Claude Code. Press c to stop waiting."),
        "{text}"
    );
    assert!(
        text.contains("● database url: postgres://mike:***@127.0.0.1:6666/mike"),
        "{text}"
    );
    assert!(
        text.contains("◇ claude route: no route is chosen. Fix: `mike setup claude`."),
        "{text}"
    );
    assert!(text.contains("○ mike-mcp:"), "{text}");
    assert!(text.contains("Fix: `mike setup claude`."), "{text}");
    Ok(())
}

#[tokio::test]
async fn the_shell_function_name_is_offered_beside_mike_claude() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let file = harness.env.config_file().to_path_buf();
    std::fs::create_dir_all(file.parent().unwrap_or(&file))?;
    std::fs::write(&file, "claude_function = \"mcc\"\n")?;
    let mut step = at(Duration::ZERO);
    step.enter(&mut harness.cx()).await;
    let text = render_step(&step);
    assert!(text.contains("with `mike claude`, or with `mcc`"), "{text}");
    Ok(())
}

#[tokio::test]
async fn the_first_request_newer_than_the_step_is_shown_when_it_arrives() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/requests"))
        .and(query_param("order", "asc"))
        .and(query_param("limit", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/requests"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([request_row(4821)])))
        .mount(&server)
        .await;
    let mut harness = Harness::new()?;
    harness.config.inspect_listen_addr = server.address().to_string();
    harness.client = ApiClient::new(&harness.config);
    let mut step = at(Duration::ZERO);
    step.enter(&mut harness.cx()).await;
    step.tick(&mut harness.cx()).await;
    step.tick(&mut harness.cx()).await;
    assert_eq!(step.wait, Wait::Waiting);
    step.tick(&mut harness.cx()).await;
    assert!(matches!(step.wait, Wait::Arrived(ref r) if r.id == 4821));
    let text = render_step(&step);
    assert!(
        text.contains("The first captured request is in the proxy:"),
        "{text}"
    );
    assert!(
        text.contains("Request 4821 arrived at 2026-10-16T21:30:00Z: POST /v1/messages"),
        "{text}"
    );
    assert!(text.contains("claude-sonnet-5-5, complete,"), "{text}");
    assert!(text.contains("session abc-123, project demo."), "{text}");
    let asked = server.received_requests().await.unwrap_or_default();
    let query = asked
        .first()
        .map(|r| r.url.query().unwrap_or_default().to_string())
        .unwrap_or_default();
    assert!(query.starts_with("date_from="), "{query}");
    assert!(query.ends_with("Z&order=asc&limit=1"), "{query}");
    Ok(())
}

#[tokio::test]
async fn polling_is_throttled_and_stops_once_the_wait_is_cancelled() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/requests"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let mut harness = Harness::new()?;
    harness.config.inspect_listen_addr = server.address().to_string();
    harness.client = ApiClient::new(&harness.config);
    let mut step = at(Duration::from_secs(3600));
    step.enter(&mut harness.cx()).await;
    for _ in 0..5 {
        step.tick(&mut harness.cx()).await;
    }
    assert_eq!(
        server.received_requests().await.unwrap_or_default().len(),
        1,
        "one poll per interval"
    );
    assert_eq!(
        step.key(key(KeyCode::Char('c')), &mut harness.cx()).await,
        Outcome::Stay
    );
    assert_eq!(step.wait, Wait::Stopped);
    let mut eager = at(Duration::ZERO);
    eager.enter(&mut harness.cx()).await;
    eager.key(key(KeyCode::Char('c')), &mut harness.cx()).await;
    let before = server.received_requests().await.unwrap_or_default().len();
    eager.tick(&mut harness.cx()).await;
    assert_eq!(
        server.received_requests().await.unwrap_or_default().len(),
        before,
        "no poll while stopped"
    );
    assert!(
        render_step(&eager).contains("Stopped waiting. Press r to check again and wait again.")
    );
    eager.key(key(KeyCode::Char('r')), &mut harness.cx()).await;
    assert_eq!(eager.wait, Wait::Waiting);
    Ok(())
}

#[tokio::test]
async fn a_proxy_that_does_not_answer_leaves_the_step_waiting() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let mut step = at(Duration::ZERO);
    step.enter(&mut harness.cx()).await;
    step.tick(&mut harness.cx()).await;
    assert_eq!(step.wait, Wait::Waiting);
    Ok(())
}

#[tokio::test]
async fn the_buttons_open_the_web_ui_and_the_tour_and_enter_starts_claude() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let mut step = at(Duration::ZERO);
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Char('b')), &mut harness.cx()).await;
    step.key(key(KeyCode::Char('t')), &mut harness.cx()).await;
    let urls = opened();
    assert!(
        urls.contains(&"http://localhost:9990/".to_string()),
        "{urls:?}"
    );
    assert!(
        urls.contains(&"http://localhost:9990/?tour=1".to_string()),
        "{urls:?}"
    );
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Leave(Nav::LaunchClaude)
    );
    assert_eq!(
        step.key(key(KeyCode::Left), &mut harness.cx()).await,
        Outcome::Back
    );
    Ok(())
}

#[tokio::test]
async fn the_hints_follow_the_wait() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let mut step = at(Duration::ZERO);
    step.enter(&mut harness.cx()).await;
    assert_eq!(step.hints().last().copied(), Some(("c", "stop")));
    step.key(key(KeyCode::Char('c')), &mut harness.cx()).await;
    assert_eq!(step.hints().last().copied(), Some(("r", "again")));
    Ok(())
}
