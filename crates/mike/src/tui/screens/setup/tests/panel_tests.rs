use crossterm::event::KeyCode;

use super::super::harness::{key, typed};
use super::super::panel::{Choice, Input, Panel, PanelEvent};
use crate::finding::Finding;
use crate::tui::test_support::render_to_string;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum A {
    One,
    Two,
    Three,
}

fn panel() -> Panel<A> {
    Panel::new(
        vec!["tool a: /bin/a".to_string()],
        vec![
            Choice::new(A::One, "First", "Does the first thing.").command("mike one"),
            Choice::new(A::Two, "Second", "Does the second thing.")
                .recommended()
                .command("mike two")
                .root_command("sudo do-two"),
            Choice::new(A::Three, "Third", "Takes a name.")
                .input(Input::new("name", "type a name")),
        ],
    )
}

fn press(panel: &mut Panel<A>, code: KeyCode) -> PanelEvent {
    panel.handle(key(code))
}

#[test]
fn the_cursor_starts_on_the_recommended_choice() {
    let panel = panel();
    assert_eq!(panel.cursor, 1);
    assert_eq!(panel.selected().map(|c| c.action), Some(A::Two));
}

#[test]
fn j_and_k_wrap_and_clear_an_earlier_result() {
    let mut panel = panel();
    panel.record(vec![Finding::ok("done")]);
    assert!(panel.done);
    press(&mut panel, KeyCode::Char('j'));
    assert_eq!(panel.cursor, 2);
    assert!(!panel.done);
    assert!(panel.result.is_empty());
    press(&mut panel, KeyCode::Down);
    assert_eq!(panel.cursor, 0);
    press(&mut panel, KeyCode::Char('k'));
    assert_eq!(panel.cursor, 2);
}

#[test]
fn enter_runs_then_continues_once_the_checks_passed() {
    let mut panel = panel();
    assert_eq!(press(&mut panel, KeyCode::Enter), PanelEvent::Run);
    panel.record(vec![Finding::failed("no")]);
    assert!(!panel.done);
    assert_eq!(press(&mut panel, KeyCode::Enter), PanelEvent::Run);
    panel.record(vec![Finding::ok("yes"), Finding::note("fyi")]);
    assert!(panel.done);
    assert_eq!(press(&mut panel, KeyCode::Enter), PanelEvent::Next);
}

#[test]
fn an_empty_result_never_counts_as_done() {
    let mut panel = panel();
    panel.record(Vec::new());
    assert!(!panel.done);
}

#[test]
fn a_shown_result_does_not_finish_the_step() {
    let mut panel = panel();
    panel.show(vec![Finding::note("nothing was run")]);
    assert!(!panel.done);
    assert_eq!(press(&mut panel, KeyCode::Enter), PanelEvent::Run);
}

#[test]
fn letters_typed_into_a_field_never_trigger_the_step_keys() {
    let mut panel = panel();
    press(&mut panel, KeyCode::Char('j'));
    assert_eq!(
        press(&mut panel, KeyCode::Enter),
        PanelEvent::Idle,
        "empty field starts typing"
    );
    assert!(panel.editing);
    for event in typed("sk-or-fqjrse") {
        assert_eq!(panel.handle(event), PanelEvent::Idle);
    }
    assert_eq!(panel.input_value(), "sk-or-fqjrse");
    press(&mut panel, KeyCode::Backspace);
    assert_eq!(panel.input_value(), "sk-or-fqjrs");
    assert_eq!(press(&mut panel, KeyCode::Enter), PanelEvent::Idle);
    assert!(!panel.editing);
    assert_eq!(press(&mut panel, KeyCode::Enter), PanelEvent::Run);
}

#[test]
fn credentials_accept_navigation_letters_and_unicode_literally() {
    let value = "sq? dollars$ quotes'\" backtick` slash\\ hash# 雪";
    let mut panel = panel();
    press(&mut panel, KeyCode::Down);
    press(&mut panel, KeyCode::Char('e'));
    for event in typed(value) {
        assert_eq!(panel.handle(event), PanelEvent::Idle);
    }
    assert_eq!(panel.input_value(), value);
    assert!(panel.editing);
}

#[test]
fn escape_stops_typing_and_keeps_the_text() {
    let mut panel = panel();
    press(&mut panel, KeyCode::Char('j'));
    press(&mut panel, KeyCode::Char('e'));
    assert!(panel.editing);
    for event in typed("abc") {
        panel.handle(event);
    }
    press(&mut panel, KeyCode::Esc);
    assert!(!panel.editing);
    assert_eq!(panel.input_value(), "abc");
}

#[test]
fn e_does_nothing_on_a_choice_without_a_field() {
    let mut panel = panel();
    press(&mut panel, KeyCode::Char('e'));
    assert!(!panel.editing);
}

#[test]
fn the_other_keys_map_to_events() {
    let mut panel = panel();
    assert_eq!(press(&mut panel, KeyCode::Char('s')), PanelEvent::Skip);
    assert_eq!(press(&mut panel, KeyCode::Char('r')), PanelEvent::Recheck);
    assert_eq!(
        press(&mut panel, KeyCode::Char('f')),
        PanelEvent::FinishLater
    );
    assert_eq!(press(&mut panel, KeyCode::Left), PanelEvent::Back);
    assert_eq!(press(&mut panel, KeyCode::Char('x')), PanelEvent::Idle);
}

#[test]
fn hints_follow_the_state() {
    let mut panel = panel();
    let keys = |p: &Panel<A>| p.hints().iter().map(|(k, _)| *k).collect::<Vec<_>>();
    assert_eq!(keys(&panel), vec!["⏎", "s", "←", "f"]);
    panel.record(vec![Finding::ok("x")]);
    assert_eq!(keys(&panel), vec!["⏎", "r", "←", "f"]);
    let mut typing = Panel::new(
        Vec::new(),
        vec![Choice::new(A::One, "One", "x").input(Input::new("n", "p"))],
    );
    typing.editing = true;
    assert_eq!(keys(&typing), vec!["⏎", "Esc"]);
}

fn draw(panel: &Panel<A>, width: u16, height: u16) -> String {
    render_to_string(width, height, |frame, area| panel.draw(frame, area, "demo"))
}

#[test]
fn the_panel_shows_found_choices_the_command_and_what_was_checked() {
    let mut panel = panel();
    let text = draw(&panel, 100, 40);
    assert!(text.contains("DEMO"), "{text}");
    assert!(text.contains("Found"), "{text}");
    assert!(text.contains("tool a: /bin/a"), "{text}");
    assert!(text.contains("Second (recommended)"), "{text}");
    assert!(text.contains("mike two"), "{text}");
    assert!(
        !text.contains("mike one"),
        "only the chosen command is shown: {text}"
    );
    assert!(
        text.contains("Run these yourself (mike never runs sudo)"),
        "{text}"
    );
    assert!(text.contains("sudo do-two"), "{text}");
    assert!(!text.contains("What was checked"), "{text}");
    panel.record(vec![
        Finding::ok("checked a"),
        Finding::note("note b"),
        Finding::failed("failed c"),
    ]);
    let text = draw(&panel, 100, 40);
    assert!(text.contains("What was checked"), "{text}");
    assert!(text.contains("● checked a"), "{text}");
    assert!(text.contains("◇ note b"), "{text}");
    assert!(text.contains("○ failed c"), "{text}");
}

#[test]
fn a_secret_field_is_masked_and_a_plain_one_is_not() {
    let mut secret = Panel::new(
        Vec::new(),
        vec![
            Choice::new(A::One, "Key", "x").input(Input::new("key", "paste").with_value("abcdef")),
        ],
    );
    if let Some(input) = secret.selected_mut().and_then(|c| c.input.as_mut()) {
        input.secret = true;
    }
    let text = draw(&secret, 80, 20);
    assert!(text.contains("******"), "{text}");
    assert!(!text.contains("abcdef"), "{text}");
    let plain = Panel::new(
        Vec::new(),
        vec![
            Choice::new(A::One, "Url", "x").input(Input::new("url", "paste").with_value("abcdef")),
        ],
    );
    assert!(draw(&plain, 80, 20).contains("abcdef"));
}

#[test]
fn long_content_scrolls_so_the_result_stays_visible() {
    let mut panel = panel();
    panel.found = (0..30).map(|n| format!("line {n}")).collect();
    panel.record(vec![Finding::ok("the last finding")]);
    let text = draw(&panel, 80, 16);
    assert!(text.contains("the last finding"), "{text}");
}

#[test]
fn text_wraps_instead_of_being_cut() {
    let panel = Panel::new(
        vec!["word ".repeat(40)],
        vec![Choice::new(A::One, "One", "why ".repeat(40))],
    );
    let lines = panel.lines(40);
    assert!(lines.len() > 10);
    for line in &lines {
        let width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        assert!(width <= 40, "{width}");
    }
}
