use crossterm::event::KeyCode;

use super::super::harness::{Harness, key, typed};
use super::super::steps::{self, welcome::Welcome};
use super::super::{DONE, SetupState, UNFINISHED, handle_key, marked_unfinished};
use crate::tui::screens::Nav;
use crate::tui::test_support::render_to_string;

#[test]
fn the_steps_run_in_the_agreed_order() {
    let labels: Vec<&str> = steps::all().iter().map(|s| s.label()).collect();
    assert_eq!(
        labels,
        vec![
            "welcome",
            "machine",
            "database",
            "service",
            "claude code",
            "features",
            "models",
            "search",
            "rerank",
            "terminal",
            "ready"
        ]
    );
}

#[test]
fn every_doctor_step_name_opens_a_step_except_the_ones_not_built_yet() {
    for name in [
        "database",
        "service",
        "claude",
        "models",
        "embeddings",
        "terminal",
    ] {
        assert!(SetupState::at_step(name).is_ok(), "{name}");
    }
    assert!(SetupState::at_step("  Database ").is_ok());
}

#[test]
fn an_unknown_step_lists_the_known_ones() {
    let error = SetupState::at_step("nope").err().unwrap_or_default();
    assert!(error.contains("`nope` is not a setup step"), "{error}");
    for name in [
        "welcome", "machine", "database", "service", "claude", "terminal", "ready",
    ] {
        assert!(error.contains(name), "{error}");
    }
}

#[test]
fn the_unfinished_marker_is_read_case_insensitively() {
    assert!(marked_unfinished(Some("unfinished")));
    assert!(marked_unfinished(Some(" Unfinished ")));
    assert!(!marked_unfinished(Some("done")));
    assert!(!marked_unfinished(Some("")));
    assert!(!marked_unfinished(None));
}

fn render(state: &mut SetupState) -> String {
    render_to_string(100, 30, |frame, area| {
        super::super::draw(frame, area, state)
    })
}

#[tokio::test]
async fn entering_the_wizard_marks_setup_unfinished_once_and_never_overwrites_done()
-> anyhow::Result<()> {
    let harness = Harness::new()?;
    let config_file = harness.env.config_file().to_path_buf();
    let (mut state, client, mut config, _dir) =
        harness.into_state(vec![Box::new(Welcome::new())], 0);
    state.on_enter_step(&client, &mut config).await;
    assert_eq!(
        std::fs::read_to_string(&config_file)?,
        "setup = \"unfinished\"\n"
    );
    std::fs::write(&config_file, "setup = \"done\"\n")?;
    state.started = false;
    state.on_enter_step(&client, &mut config).await;
    assert_eq!(std::fs::read_to_string(&config_file)?, "setup = \"done\"\n");
    Ok(())
}

#[tokio::test]
async fn finishing_later_leaves_the_answers_and_the_unfinished_marker() -> anyhow::Result<()> {
    let harness = Harness::new()?;
    let config_file = harness.env.config_file().to_path_buf();
    std::fs::create_dir_all(config_file.parent().unwrap_or(&config_file))?;
    std::fs::write(&config_file, "launch_backend = \"herdr\"\n")?;
    let (mut state, client, mut config, _dir) =
        harness.into_state(vec![Box::new(Welcome::new())], 0);
    state.on_enter_step(&client, &mut config).await;
    let nav = handle_key(&mut state, key(KeyCode::Char('f')), &client, &mut config).await;
    assert_eq!(nav, Nav::SetupDone);
    let text = std::fs::read_to_string(&config_file)?;
    assert!(text.contains("launch_backend = \"herdr\""), "{text}");
    assert!(
        text.contains(&format!("setup = \"{UNFINISHED}\"")),
        "{text}"
    );
    Ok(())
}

#[tokio::test]
async fn escape_goes_home_and_q_quits_unless_a_field_is_being_typed_into() -> anyhow::Result<()> {
    let harness = Harness::new()?;
    let (mut state, client, mut config, _dir) =
        harness.into_state(vec![Box::new(steps::database::Database::new())], 0);
    state.on_enter_step(&client, &mut config).await;
    assert_eq!(
        handle_key(&mut state, key(KeyCode::Esc), &client, &mut config).await,
        Nav::SetupDone
    );
    assert_eq!(
        handle_key(&mut state, key(KeyCode::Char('q')), &client, &mut config).await,
        Nav::Quit
    );
    handle_key(&mut state, key(KeyCode::Char('j')), &client, &mut config).await;
    handle_key(&mut state, key(KeyCode::Enter), &client, &mut config).await;
    assert!(state.steps[0].editing());
    for event in typed("qsf") {
        assert_eq!(
            handle_key(&mut state, event, &client, &mut config).await,
            Nav::None
        );
    }
    assert!(
        state.steps[0].editing(),
        "typing q, s and f must not leave the field"
    );
    Ok(())
}

#[tokio::test]
async fn reaching_the_last_step_records_done_and_leaving_it_goes_home() -> anyhow::Result<()> {
    let harness = Harness::new()?;
    let config_file = harness.env.config_file().to_path_buf();
    let (mut state, client, mut config, _dir) = harness.into_state(
        vec![
            Box::new(Welcome::new()),
            Box::new(steps::machine::Machine::new()),
        ],
        0,
    );
    state.on_enter_step(&client, &mut config).await;
    handle_key(&mut state, key(KeyCode::Enter), &client, &mut config).await;
    assert_eq!(state.current, 1);
    let text = std::fs::read_to_string(&config_file)?;
    assert!(text.contains(&format!("setup = \"{DONE}\"")), "{text}");
    let nav = handle_key(&mut state, key(KeyCode::Enter), &client, &mut config).await;
    assert_eq!(nav, Nav::SetupDone);
    handle_key(&mut state, key(KeyCode::Left), &client, &mut config).await;
    assert_eq!(state.current, 0);
    Ok(())
}

#[tokio::test]
async fn declining_the_look_is_remembered_for_the_later_steps() -> anyhow::Result<()> {
    let harness = Harness::new()?;
    let (mut state, client, mut config, _dir) = harness.into_state(
        vec![
            Box::new(Welcome::new()),
            Box::new(steps::machine::Machine::new()),
        ],
        0,
    );
    state.on_enter_step(&client, &mut config).await;
    handle_key(&mut state, key(KeyCode::Down), &client, &mut config).await;
    handle_key(&mut state, key(KeyCode::Enter), &client, &mut config).await;
    assert!(!state.env.looked);
    let text = render(&mut state);
    assert!(
        text.contains("You chose not to look at this machine"),
        "{text}"
    );
    Ok(())
}

#[tokio::test]
async fn the_indicator_and_status_bar_name_the_step_and_its_number() -> anyhow::Result<()> {
    let all = steps::all();
    let total = all.len();
    let at = |label: &str| all.iter().position(|s| s.label() == label).unwrap_or(total);
    let (database, machine, claude) = (at("database"), at("machine"), at("claude code"));
    let parts = all
        .get(claude)
        .and_then(|s| s.progress())
        .map_or(0, |(_, of)| of);
    let harness = Harness::new()?;
    let (mut state, client, mut config, _dir) = harness.into_state(steps::all(), database);
    let _ = (&client, &mut config);
    let text = render(&mut state);
    assert!(
        text.contains(&format!("{}·database", database + 1)),
        "{text}"
    );
    assert!(text.contains(&format!("{}·machine", machine + 1)), "{text}");
    assert!(
        text.contains(&format!("step {}/{total} · database", database + 1)),
        "{text}"
    );
    assert!(text.contains("SETUP"), "{text}");
    state.current = claude;
    let text = render(&mut state);
    assert!(
        text.contains(&format!("step {}/{total} · part 1/{parts}", claude + 1)),
        "{text}"
    );
    state.current = total - 1;
    let text = render(&mut state);
    assert!(text.contains("READY"), "{text}");
    Ok(())
}

#[tokio::test]
async fn every_step_draws_at_a_small_size_without_panicking() -> anyhow::Result<()> {
    let harness = Harness::new()?;
    let count = steps::all().len();
    let (mut state, _client, _config, _dir) = harness.into_state(steps::all(), 0);
    for index in 0..count {
        state.current = index;
        let text = render_to_string(60, 18, |frame, area| {
            super::super::draw(frame, area, &mut state)
        });
        assert!(
            text.contains(&format!("step {}/{count}", index + 1)),
            "{text}"
        );
    }
    Ok(())
}
