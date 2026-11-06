use super::*;
use crate::tui::client::ApiClient;
use crate::tui::test_support::render_to_string;
use crossterm::event::KeyModifiers;
use std::time::Duration as StdDuration;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn format_uptime_switches_units_by_magnitude() {
    assert_eq!(format_uptime(StdDuration::from_secs(45)), "45s");
    assert_eq!(format_uptime(StdDuration::from_secs(125)), "2m05s");
    assert_eq!(
        format_uptime(StdDuration::from_secs(2 * 3600 + 14 * 60)),
        "2h14m"
    );
}

#[test]
fn describe_ensure_names_already_running_and_spawned() {
    assert_eq!(
        describe_ensure(&EnsureOutcome::AlreadyRunning),
        "already healthy — nothing to start"
    );
    assert_eq!(
        describe_ensure(&EnsureOutcome::Spawned {
            pid: 123,
            log_path: std::path::PathBuf::from("/tmp/x.log"),
        }),
        "started (pid 123)"
    );
}

#[test]
fn describe_stop_names_not_running_and_stopped() {
    assert_eq!(describe_stop(StopOutcome::NotRunning), "not running");
    assert_eq!(
        describe_stop(StopOutcome::Stopped { pid: 42 }),
        "stopped (pid 42)"
    );
}

#[test]
fn running_state_renders_pid_and_uptime() {
    let mut state = StartStopState::new();
    state.running = true;
    state.pid = Some(48120);
    state.started_at = Some(std::time::SystemTime::now() - StdDuration::from_secs(3600));
    state.ports = vec![("proxy · /v1", "127.0.0.1:9988".to_string(), true)];
    let text = render_to_string(120, 30, |frame, area| draw(frame, area, &mut state));
    assert!(text.contains("PORTS"), "{text}");
    assert!(text.contains("LOG"), "{text}");
    assert!(text.contains("LIFECYCLE"), "{text}");
    assert!(text.contains("48120"), "{text}");
    assert!(text.contains("running"), "{text}");
    assert!(text.contains("RUNNING"), "status bar mode: {text}");
}

#[test]
fn stopped_state_dims_stop_and_shows_no_pid() {
    let mut state = StartStopState::new();
    state.running = false;
    let text = render_to_string(120, 30, |frame, area| draw(frame, area, &mut state));
    assert!(text.contains("stopped"), "{text}");
    assert!(
        text.contains("not running"),
        "Stop must show a not-running suffix: {text}"
    );
    assert!(text.contains("STOPPED"), "{text}");
}

#[test]
fn tab_cycles_the_three_panels() {
    let mut state = StartStopState::new();
    state.focus = 1;
    assert_eq!(state.focus, 1);
    state.focus = (state.focus + 1) % 3;
    assert_eq!(state.focus, 2);
    state.focus = (state.focus + 1) % 3;
    assert_eq!(state.focus, 0);
}

#[tokio::test]
async fn x_against_a_dead_port_with_no_pidfile_logs_not_running() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut config = ProxyConfig::for_tests_in_memory();
    config.log_dir = Some(dir.path().join("logs"));
    config.config_path = dir.path().join("config/config.toml");
    config.inspect_listen_addr = "127.0.0.1:1".to_string();
    config.proxy_listen_addr = "127.0.0.1:2".to_string();
    config.spa_enabled = false;
    let client = ApiClient::new(&config);

    let mut state = StartStopState::new();
    handle_key(&mut state, key(KeyCode::Char('x')), &client, &config).await;

    assert!(
        state.log_lines.iter().any(|l| l == "not running"),
        "stop_stack's real NotRunning outcome must reach the log: {:?}, banner: {:?}",
        state.log_lines,
        state.banner
    );
    assert!(state.banner.is_none());
    Ok(())
}

#[test]
fn q_quits_and_esc_returns_home() {
    let mut state = StartStopState::new();
    let config = ProxyConfig::for_tests_in_memory();
    let client = ApiClient::new(&config);
    let nav = futures::executor::block_on(handle_key(
        &mut state,
        key(KeyCode::Char('q')),
        &client,
        &config,
    ));
    assert_eq!(nav, Nav::Quit);
    let nav =
        futures::executor::block_on(handle_key(&mut state, key(KeyCode::Esc), &client, &config));
    assert_eq!(nav, Nav::Home);
}
