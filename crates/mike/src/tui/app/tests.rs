use super::*;
use crate::tui::test_support::{render_to_lines, render_to_string};
use crossterm::event::KeyModifiers;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl_slash() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('/'), KeyModifiers::CONTROL)
}

#[test]
fn setup_text_entry_keeps_question_mark_literal_and_ctrl_slash_available() {
    assert!(!is_shortcuts_toggle(&key(KeyCode::Char('?')), true));
    assert!(is_shortcuts_toggle(&ctrl_slash(), true));
    assert!(is_shortcuts_toggle(&key(KeyCode::Char('?')), false));
}

#[test]
fn new_app_lands_on_setup_when_unconfigured_and_home_when_configured() {
    let mut unconfigured = ProxyConfig::for_tests_in_memory();
    unconfigured.persist_enabled = true;
    unconfigured.database_url = None;
    assert!(matches!(App::new(unconfigured).screen, Screen::Setup(_)));

    let mut configured = ProxyConfig::for_tests_in_memory();
    configured.persist_enabled = true;
    configured.database_url = Some("postgres://127.0.0.1:5432/mike".to_string());
    assert!(matches!(App::new(configured).screen, Screen::Home(_)));
}

#[tokio::test]
async fn question_mark_and_ctrl_slash_both_toggle_the_shortcuts_overlay() {
    let mut config = ProxyConfig::for_tests_in_memory();
    config.database_url = Some("postgres://127.0.0.1:5432/mike".to_string());
    let mut app = App::new(config);
    assert!(!app.show_shortcuts);

    app.handle_key(key(KeyCode::Char('?'))).await;
    assert!(app.show_shortcuts);
    app.handle_key(key(KeyCode::Char('?'))).await;
    assert!(!app.show_shortcuts);

    app.handle_key(ctrl_slash()).await;
    assert!(app.show_shortcuts);
    app.handle_key(ctrl_slash()).await;
    assert!(!app.show_shortcuts);
}

#[tokio::test]
async fn overlay_absorbs_keys_except_esc() {
    let mut config = ProxyConfig::for_tests_in_memory();
    config.database_url = Some("postgres://127.0.0.1:5432/mike".to_string());
    let mut app = App::new(config);

    app.handle_key(key(KeyCode::Char('?'))).await;
    assert!(app.show_shortcuts);
    app.handle_key(key(KeyCode::Char('q'))).await;
    assert!(app.show_shortcuts, "the modal must still be open");
    assert!(
        !app.should_quit,
        "the absorbed 'q' must not have quit the app"
    );

    app.handle_key(key(KeyCode::Esc)).await;
    assert!(!app.show_shortcuts, "Esc must close the modal");
}

#[tokio::test]
async fn draw_does_not_panic_on_any_screen_with_or_without_the_overlay() {
    let mut config = ProxyConfig::for_tests_in_memory();
    config.database_url = Some("postgres://127.0.0.1:5432/mike".to_string());
    let mut app = App::new(config);
    let _ = render_to_string(100, 30, |frame, _area| app.draw(frame));

    app.handle_key(key(KeyCode::Char('?'))).await;
    let _ = render_to_string(100, 30, |frame, _area| app.draw(frame));
}

#[tokio::test]
async fn top_bar_is_on_every_nav_screen_and_the_rail_does_not_stretch() {
    let mut config = ProxyConfig::for_tests_in_memory();
    config.database_url = Some("postgres://127.0.0.1:5432/mike".to_string());
    let mut app = App::new(config);

    let lines = render_to_lines(140, 40, |frame, _area| app.draw(frame));
    assert!(lines[0].contains("mike"), "no top bar on Home: {lines:?}");
    assert!(
        lines[0].contains("online") || lines[0].contains("stopped"),
        "top bar missing its liveness word: {lines:?}"
    );
    let row_after_rail = &lines[1 + widgets::NAV_HEIGHT as usize];
    assert!(
        !row_after_rail[..widgets::NAV_WIDTH as usize].contains(['│', '┌', '└', '┐', '┘']),
        "the rail must not stretch past its own content height: {row_after_rail:?}"
    );

    app.handle_key(key(KeyCode::Char('j'))).await;
    let lines = render_to_lines(140, 40, |frame, _area| app.draw(frame));
    assert!(lines[0].contains("mike"), "no top bar on Config: {lines:?}");

    app.handle_key(key(KeyCode::Char('j'))).await;
    let lines = render_to_lines(140, 40, |frame, _area| app.draw(frame));
    assert!(lines[0].contains("mike"), "no top bar on Setup: {lines:?}");
}

#[tokio::test]
async fn nav_focused_arrow_keys_switch_screens_without_resetting_selection() {
    let mut config = ProxyConfig::for_tests_in_memory();
    config.database_url = Some("postgres://127.0.0.1:5432/mike".to_string());
    let mut app = App::new(config);
    assert!(matches!(app.screen, Screen::Home(_)));
    assert!(app.nav_focus);

    app.handle_key(key(KeyCode::Char('j'))).await;
    assert!(matches!(app.screen, Screen::Config(_)));
    assert_eq!(app.nav.selected(), Some(1));
    assert!(app.nav_focus, "still browsing the rail, not inside Config");

    app.handle_key(key(KeyCode::Char('j'))).await;
    assert!(matches!(app.screen, Screen::Setup(_)));
    assert_eq!(app.nav.selected(), Some(2));

    app.handle_key(key(KeyCode::Char('k'))).await;
    assert!(matches!(app.screen, Screen::Config(_)));
}

#[tokio::test]
async fn g_and_shift_g_jump_to_the_ends_of_the_rail() {
    let mut config = ProxyConfig::for_tests_in_memory();
    config.database_url = Some("postgres://127.0.0.1:5432/mike".to_string());
    let mut app = App::new(config);

    app.handle_key(key(KeyCode::Char('G'))).await;
    assert_eq!(app.nav.selected(), Some(3));
    assert!(matches!(app.screen, Screen::Sessions(_)));

    app.handle_key(key(KeyCode::Char('g'))).await;
    assert_eq!(app.nav.selected(), Some(0));
    assert!(matches!(app.screen, Screen::Home(_)));
}

#[tokio::test]
async fn nav_rail_reaches_sessions_at_the_fourth_position() {
    let mut config = ProxyConfig::for_tests_in_memory();
    config.database_url = Some("postgres://127.0.0.1:5432/mike".to_string());
    let mut app = App::new(config);

    app.handle_key(key(KeyCode::Char('j'))).await;
    app.handle_key(key(KeyCode::Char('j'))).await;
    app.handle_key(key(KeyCode::Char('j'))).await;
    assert_eq!(app.nav.selected(), Some(3));
    assert!(matches!(app.screen, Screen::Sessions(_)));

    app.handle_key(key(KeyCode::Char('k'))).await;
    assert!(matches!(app.screen, Screen::Setup(_)));
}

#[tokio::test]
async fn tab_hands_focus_to_content_and_back_tab_returns_it() {
    let mut config = ProxyConfig::for_tests_in_memory();
    config.database_url = Some("postgres://127.0.0.1:5432/mike".to_string());
    let mut app = App::new(config);

    app.handle_key(key(KeyCode::Tab)).await;
    assert!(!app.nav_focus);
    app.handle_key(key(KeyCode::Char('j'))).await;
    assert!(matches!(app.screen, Screen::Home(_)));

    app.handle_key(key(KeyCode::BackTab)).await;
    assert!(app.nav_focus);
}

#[tokio::test]
async fn esc_from_config_content_returns_to_home_with_the_rail_immediately_browsable() {
    let mut config = ProxyConfig::for_tests_in_memory();
    config.database_url = Some("postgres://127.0.0.1:5432/mike".to_string());
    let mut app = App::new(config);

    app.handle_key(key(KeyCode::Char('j'))).await;
    assert!(matches!(app.screen, Screen::Config(_)));
    app.handle_key(key(KeyCode::Tab)).await;
    assert!(!app.nav_focus);

    app.handle_key(key(KeyCode::Esc)).await;
    assert!(matches!(app.screen, Screen::Home(_)));
    assert!(
        app.nav_focus,
        "landing back on Home via Esc must restore rail focus, or the \
         operator is stranded until they discover the undocumented \
         Shift+Tab"
    );

    app.handle_key(key(KeyCode::Char('j'))).await;
    assert!(matches!(app.screen, Screen::Config(_)));
}

#[tokio::test]
async fn draw_does_not_panic_after_navigating_to_config_or_setup() {
    let mut config = ProxyConfig::for_tests_in_memory();
    config.database_url = Some("postgres://127.0.0.1:5432/mike".to_string());
    let mut app = App::new(config);

    app.handle_key(key(KeyCode::Char('j'))).await;
    let _ = render_to_string(100, 30, |frame, _area| app.draw(frame));

    app.handle_key(key(KeyCode::Char('j'))).await;
    let _ = render_to_string(100, 30, |frame, _area| app.draw(frame));
}
