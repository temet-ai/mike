use super::*;
use crate::tui::test_support::{render_to_lines, render_to_string};
use crossterm::event::KeyModifiers;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn setting(group: &str, key: &str, kind: SettingKind, value: serde_json::Value) -> SettingEntry {
    SettingEntry {
        key: key.to_string(),
        group: group.to_string(),
        kind,
        applies: crate::tui::client::SettingApplies::NewSessions,
        description: format!("{key} description"),
        value,
    }
}

#[test]
fn render_value_shows_on_off_for_bool_and_dash_for_unset_string() {
    let bool_on = setting("g", "k", SettingKind::Bool, serde_json::json!(true));
    assert_eq!(render_value(&bool_on).0, "on");
    let bool_off = setting("g", "k", SettingKind::Bool, serde_json::json!(false));
    assert_eq!(render_value(&bool_off).0, "off");
    let unset = setting("g", "k", SettingKind::String, serde_json::json!(""));
    assert_eq!(render_value(&unset).0, "—");
    let set = setting("g", "k", SettingKind::String, serde_json::json!("ollama"));
    assert_eq!(render_value(&set).0, "ollama");
}

#[test]
fn capability_strip_renders_a_compact_one_liner_not_full_rows() {
    let mut state = ConfigState::new();
    state.capability_summary = "3/4 capabilities active · steering".to_string();
    let text = render_to_string(100, 20, |frame, area| draw(frame, area, &mut state));
    assert!(text.contains("3/4 capabilities active"), "{text}");
    assert!(
        !text.contains("CAPABILITIES"),
        "must not render Home's full capabilities block: {text}"
    );
}

#[test]
fn openrouter_key_env_flag_renders_as_a_synthetic_steerer_row() {
    let mut state = ConfigState::new();
    state.rows = vec![Row::OpenRouterKeyEnvFlag(true)];
    let text = render_to_string(100, 20, |frame, area| draw(frame, area, &mut state));
    assert!(text.contains("STEERER"), "{text}");
    assert!(text.contains("steerer.openrouter_api_key"), "{text}");
    assert!(text.contains("set"), "{text}");
}

#[test]
fn settings_render_grouped_with_uppercase_headers() {
    let mut state = ConfigState::new();
    state.rows = vec![
        Row::Setting(setting(
            "Embedding",
            "embedding.url",
            SettingKind::String,
            serde_json::json!("http://x"),
        )),
        Row::Setting(setting(
            "Mailbox",
            "mailbox.enabled",
            SettingKind::Bool,
            serde_json::json!(true),
        )),
    ];
    let text = render_to_string(100, 20, |frame, area| draw(frame, area, &mut state));
    assert!(text.contains("EMBEDDING"), "{text}");
    assert!(text.contains("MAILBOX"), "{text}");
    assert!(text.contains("embedding.url"), "{text}");
}

#[test]
fn a_key_longer_than_the_key_column_still_gets_a_gap_before_its_value() -> anyhow::Result<()> {
    let mut state = ConfigState::new();
    state.rows = vec![Row::Setting(setting(
        "Recall injector",
        "recall_injector.max_lessons_per_topic",
        SettingKind::Int,
        serde_json::json!(3),
    ))];
    let lines = render_to_lines(100, 10, |frame, area| draw(frame, area, &mut state));
    let Some(row) = lines
        .iter()
        .find(|l| l.contains("recall_injector.max_lessons_per_topic"))
    else {
        anyhow::bail!("the overflowing row must render: {lines:?}");
    };
    assert!(
        row.contains("max_lessons_per_topic 3") || row.contains("max_lessons_per_topic  3"),
        "an overflowing key must still have a gap before its value: {row:?}"
    );
    Ok(())
}

#[test]
fn j_k_g_shift_g_move_selection_within_bounds() {
    let mut state = ConfigState::new();
    state.rows = (0..5)
        .map(|i| {
            Row::Setting(setting(
                "g",
                &format!("k{i}"),
                SettingKind::Bool,
                serde_json::json!(true),
            ))
        })
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
fn scroll_keeps_the_selected_row_visible_when_the_list_exceeds_the_viewport() {
    let mut state = ConfigState::new();
    state.rows = (0..40)
        .map(|i| {
            Row::Setting(setting(
                "g",
                &format!("setting.k{i}"),
                SettingKind::Bool,
                serde_json::json!(true),
            ))
        })
        .collect();
    state.selected = 39;
    let text = render_to_string(80, 10, |frame, area| draw(frame, area, &mut state));
    assert!(
        text.contains("setting.k39"),
        "the selected row must scroll into view: {text}"
    );
    assert!(
        !text.contains("setting.k0 "),
        "far-above rows must scroll out of view, not render alongside the selection: {text}"
    );
}

#[test]
fn q_quits_and_esc_returns_home() {
    let mut state = ConfigState::new();
    let client = test_client();
    let nav = futures::executor::block_on(handle_key(&mut state, key(KeyCode::Char('q')), &client));
    assert_eq!(nav, Nav::Quit);
    let nav = futures::executor::block_on(handle_key(&mut state, key(KeyCode::Esc), &client));
    assert_eq!(nav, Nav::Home);
}

fn test_client() -> ApiClient {
    ApiClient::new(&crate::config::ProxyConfig::for_tests_in_memory())
}
