use super::*;
use crate::tui::client::CapabilityState;
use crate::tui::test_support::render_to_string;
use crossterm::event::KeyModifiers;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn test_config() -> ProxyConfig {
    ProxyConfig::for_tests_in_memory()
}

fn healthy_state() -> HomeState {
    let mut state = HomeState::new();
    state.binds = vec![
        ("proxy · /v1", "127.0.0.1:9988".to_string(), true),
        ("inspect · /api", "127.0.0.1:9989".to_string(), true),
        ("web ui", "127.0.0.1:9990".to_string(), true),
    ];
    state.capabilities = Some(vec![CapabilityRow {
        id: "steering".to_string(),
        label: "Steering & judges".to_string(),
        state: CapabilityState::Inert,
        detail: String::new(),
        fix: String::new(),
    }]);
    state.mcp_on_path = true;
    state.running = true;
    state.pid = Some(1234);
    state.started_at = Some(std::time::SystemTime::now());
    state.memory = Some(MemoryUsagePayload {
        entry_count: 42,
        entries_total_bytes: 1024 * 1024,
        max_store_bytes: 10 * 1024 * 1024,
        over_byte_budget: false,
        process_rss_bytes: Some(64 * 1024 * 1024),
    });
    state
}

#[test]
fn healthy_state_renders_grouped_sections_with_one_status_vocabulary() {
    let mut state = healthy_state();
    let text = render_to_string(120, 40, |frame, area| draw(frame, area, &mut state));
    assert!(text.contains("SERVICES"), "{text}");
    assert!(text.contains("BINDS"), "{text}");
    assert!(text.contains("SYSTEM"), "{text}");
    assert!(text.contains("CAPABILITIES"), "{text}");
    assert!(text.contains("Steering & judges"), "{text}");
    assert!(text.contains("running"), "{text}");
    assert!(text.contains("pid 1234"), "{text}");
    assert!(text.contains("store"), "{text}");
    assert!(text.contains("1.0MB / 10.0MB"), "{text}");
    assert!(text.contains("process"), "{text}");
    assert!(text.contains("64.0MB (peak)"), "{text}");
    assert!(
        text.contains("mike-mcp") && text.contains("on PATH · inspect reachable"),
        "{text}"
    );
    assert!(text.contains("ONLINE"), "{text}");
    assert!(
        !text.contains('◆') && !text.contains('◇'),
        "capability rows must share the binds' circle glyphs, not diamonds: {text}"
    );
}

#[test]
fn binds_and_capabilities_share_one_vocabulary_and_align_within_each_section() {
    let mut state = HomeState::new();
    state.binds = vec![
        ("proxy · /v1", "127.0.0.1:9988".to_string(), true),
        ("inspect · /api", "127.0.0.1:9989".to_string(), true),
        ("web ui", "127.0.0.1:9990".to_string(), false),
    ];
    state.capabilities = Some(vec![
        CapabilityRow {
            id: "a".to_string(),
            label: "Alpha".to_string(),
            state: CapabilityState::Active,
            detail: String::new(),
            fix: String::new(),
        },
        CapabilityRow {
            id: "b".to_string(),
            label: "Beta longer name".to_string(),
            state: CapabilityState::Inert,
            detail: String::new(),
            fix: String::new(),
        },
        CapabilityRow {
            id: "c".to_string(),
            label: "Gamma".to_string(),
            state: CapabilityState::Degraded,
            detail: String::new(),
            fix: String::new(),
        },
        CapabilityRow {
            id: "d".to_string(),
            label: "Delta".to_string(),
            state: CapabilityState::Off,
            detail: String::new(),
            fix: String::new(),
        },
    ]);

    let lines = crate::tui::test_support::render_to_lines(120, 40, |frame, area| {
        draw(frame, area, &mut state)
    });
    let text = lines.join("\n");

    for banned in ["ACTIVE", "INERT", "DEGRADED", "OFF"] {
        assert!(
            !text.contains(banned),
            "banned word {banned:?} found: {text}"
        );
    }

    let bind_expectations = [
        ("127.0.0.1:9988", "up"),
        ("127.0.0.1:9989", "up"),
        ("127.0.0.1:9990", "down"),
    ];
    let bind_cols: Vec<usize> = bind_expectations
        .iter()
        .map(|(needle, word)| {
            let line = lines
                .iter()
                .find(|l| l.contains(needle))
                .unwrap_or_else(|| panic!("no bind row for {needle}: {lines:?}"));
            let byte_idx = line
                .find(word)
                .unwrap_or_else(|| panic!("bind row missing {word:?}: {line}"));
            terminal_column(line, byte_idx)
        })
        .collect();
    assert!(
        bind_cols.iter().all(|&c| c == bind_cols[0]),
        "BINDS rows must align their state word at the same column: {bind_cols:?} in {lines:?}"
    );

    let cap_expectations = [
        ("Alpha", "up"),
        ("Beta longer name", "inert"),
        ("Gamma", "degraded"),
        ("Delta", "off"),
    ];
    let cap_cols: Vec<usize> = cap_expectations
        .iter()
        .map(|(needle, word)| {
            let line = lines
                .iter()
                .find(|l| l.contains(needle))
                .unwrap_or_else(|| panic!("no capability row for {needle}: {lines:?}"));
            let byte_idx = line
                .find(word)
                .unwrap_or_else(|| panic!("capability row missing {word:?}: {line}"));
            terminal_column(line, byte_idx)
        })
        .collect();
    assert!(
        cap_cols.iter().all(|&c| c == cap_cols[0]),
        "CAPABILITIES rows must align their state word at the same column: {cap_cols:?} in {lines:?}"
    );
}

fn terminal_column(line: &str, byte_idx: usize) -> usize {
    line[..byte_idx].chars().count()
}

#[test]
fn narrow_width_renders_without_panicking() {
    let mut state = HomeState::new();
    let text = render_to_string(60, 30, |frame, area| draw(frame, area, &mut state));
    assert!(text.contains("stack stopped"), "{text}");
    assert!(text.contains("SERVICES"), "{text}");
}

#[test]
fn down_state_shows_down_binds_and_a_stopped_toggle_hint() {
    let mut state = HomeState::new();
    state.binds = vec![("proxy · /v1", "127.0.0.1:9988".to_string(), false)];
    let text = render_to_string(120, 30, |frame, area| draw(frame, area, &mut state));
    assert!(text.contains("down"), "{text}");
    assert!(text.contains("stopped"), "{text}");
    assert!(text.contains("[t] start"), "{text}");
}

#[test]
fn mcp_on_path_but_inspect_unreachable_says_so() {
    let mut state = HomeState::new();
    state.mcp_on_path = true;
    state.binds = vec![("inspect · /api", "127.0.0.1:9989".to_string(), false)];
    let text = render_to_string(120, 30, |frame, area| draw(frame, area, &mut state));
    assert!(
        text.contains("inspect unreachable (calls will fail)"),
        "{text}"
    );
}

#[test]
fn l_opens_the_full_service_screen() {
    let mut state = HomeState::new();
    let client = ApiClient::new(&test_config());
    let config = test_config();
    assert_eq!(
        futures::executor::block_on(handle_key(
            &mut state,
            key(KeyCode::Char('l')),
            &client,
            &config
        )),
        Nav::StartStop
    );
}

#[test]
fn q_quits_and_s_opens_setup() {
    let mut state = HomeState::new();
    let client = ApiClient::new(&test_config());
    let config = test_config();
    assert_eq!(
        futures::executor::block_on(handle_key(
            &mut state,
            key(KeyCode::Char('q')),
            &client,
            &config
        )),
        Nav::Quit
    );
    assert_eq!(
        futures::executor::block_on(handle_key(
            &mut state,
            key(KeyCode::Char('s')),
            &client,
            &config
        )),
        Nav::Setup
    );
}

#[tokio::test]
async fn t_when_running_stops_via_the_real_engine_without_spawning() {
    let mut state = HomeState::new();
    state.running = true;
    let client = ApiClient::new(&test_config());
    let config = test_config();
    handle_key(&mut state, key(KeyCode::Char('t')), &client, &config).await;
    assert!(!state.busy);
}
