use crossterm::event::KeyCode;

use super::Claude;
use super::mcp::{self, McpAction, McpInputs};
use super::mods::{self, ModsAction};
use super::route::{self, RouteInputs};
use super::shell::{self, ShellAction, ShellInputs, ShellStep};
use super::trust;
use crate::claude_code::{Registration, Route, Shell};
use crate::tui::screens::setup::facts::ToolFact;
use crate::tui::screens::setup::harness::{Harness, key, render_step, typed};
use crate::tui::screens::setup::step::{Outcome, WizardStep};

fn fact() -> Option<ToolFact> {
    Some(ToolFact {
        path: "/usr/local/bin/claude".into(),
        version: Some("2.1.0 (Claude Code)".to_string()),
    })
}

fn registration(command: Option<&str>, exists: bool) -> Result<Registration, String> {
    Ok(Registration {
        command: command.map(str::to_string),
        command_exists: exists,
        inspect_url: None,
        in_a_project: false,
    })
}

fn mcp_inputs() -> McpInputs {
    McpInputs {
        claude: fact(),
        binary: Some("/opt/homebrew/bin/mike-mcp".into()),
        registration: registration(None, false),
        brew: true,
        inspect_url: "http://127.0.0.1:9989".to_string(),
    }
}

#[test]
fn the_mcp_part_recommends_registering_and_shows_what_it_writes() {
    let panel = mcp::plan_panel(&mcp_inputs());
    let choice = panel
        .selected()
        .map(|c| (c.action, c.commands.clone(), c.why.clone()));
    let (action, commands, why) = choice.unwrap_or((McpAction::Leave, Vec::new(), String::new()));
    assert_eq!(action, McpAction::Register);
    assert_eq!(commands, vec!["mike claude-code mcp"]);
    assert!(
        why.contains("MIKE_INSPECT_URL=http://127.0.0.1:9989"),
        "{why}"
    );
    assert!(why.contains("keeps every other key"), "{why}");
    let found = panel.found.join("\n");
    assert!(
        found.contains("mike-mcp: /opt/homebrew/bin/mike-mcp"),
        "{found}"
    );
    assert!(found.contains("does not register mike yet"), "{found}");
}

#[test]
fn without_mike_mcp_the_part_offers_the_homebrew_install_or_only_to_leave() {
    let mut missing = mcp_inputs();
    missing.binary = None;
    let panel = mcp::plan_panel(&missing);
    let actions: Vec<McpAction> = panel.choices.iter().map(|c| c.action).collect();
    assert_eq!(actions, vec![McpAction::InstallWithBrew, McpAction::Leave]);
    assert_eq!(
        panel.choices[0].commands,
        vec!["brew install temet-ai/mike/mike-mcp"]
    );
    missing.brew = false;
    let actions: Vec<McpAction> = mcp::plan_panel(&missing)
        .choices
        .iter()
        .map(|c| c.action)
        .collect();
    assert_eq!(actions, vec![McpAction::Leave]);
}

#[test]
fn a_stale_registration_is_called_out_in_the_found_lines() {
    let mut stale = mcp_inputs();
    stale.registration = registration(Some("mike-mcp"), false);
    let found = mcp::plan_panel(&stale).found.join("\n");
    assert!(
        found.contains("registers mike with mike-mcp (that file does not exist)"),
        "{found}"
    );
    stale.claude = None;
    let found = mcp::plan_panel(&stale).found.join("\n");
    assert!(
        found.contains("Claude Code (`claude`) is not on PATH"),
        "{found}"
    );
}

#[test]
fn the_route_part_starts_on_the_saved_route_and_explains_both() {
    let inputs = RouteInputs {
        stored: Some(Route::Plain),
        sidecar_url: "http://127.0.0.1:9987".to_string(),
        sidecar: Err("connection refused".to_string()),
        ca: None,
    };
    let panel = route::plan_panel(&inputs);
    assert_eq!(panel.selected().map(|c| c.action), Some(Route::Plain));
    assert_eq!(panel.choices.iter().filter(|c| c.recommended).count(), 1);
    assert!(panel.choices[0].why.contains("Remote Control"));
    assert!(panel.choices[1].why.contains("needs no certificate"));
    let found = panel.found.join("\n");
    assert!(
        found.contains("does not answer on http://127.0.0.1:9987 (connection refused)"),
        "{found}"
    );
    assert!(found.contains("Saved route: plain"), "{found}");
}

fn shell_inputs(installed: &[Shell], login: Option<Shell>) -> ShellInputs {
    ShellInputs {
        installed: installed
            .iter()
            .map(|s| (*s, format!("/h/.{}rc", s.name()).into(), false))
            .collect(),
        login,
        route: Route::Sidecar,
        name: "cc".to_string(),
        proxy_addr: "127.0.0.1:9988".to_string(),
        mike: "mike".to_string(),
    }
}

#[test]
fn the_shell_part_recommends_the_login_shell_and_offers_all_installed_ones() {
    let panel = shell::plan_panel(&shell_inputs(&[Shell::Zsh, Shell::Bash], Some(Shell::Zsh)));
    let selected = panel.selected().map(|c| c.action.clone());
    assert_eq!(selected, Some(ShellAction::Add(vec![Shell::Zsh])));
    assert_eq!(panel.choices.len(), 3);
    assert!(panel.choices[1].label.contains("zsh, bash"));
    assert_eq!(panel.choices.iter().filter(|c| c.recommended).count(), 1);
    let first = &panel.choices[0];
    assert_eq!(
        first.commands,
        vec!["mike claude-code shell --shell zsh --name cc"]
    );
    assert!(first.why.contains("cc() {"), "{}", first.why);
    assert!(first.why.contains("command mike claude"), "{}", first.why);
}

#[test]
fn the_shell_part_lists_only_installed_shells_and_can_be_left_empty() {
    let none = shell::plan_panel(&shell_inputs(&[], None));
    assert_eq!(none.choices.len(), 1);
    assert_eq!(none.choices[0].action, ShellAction::Leave);
    assert!(
        none.found
            .join("\n")
            .contains("None of zsh, bash or fish was found")
    );
    let fish = shell::plan_panel(&shell_inputs(&[Shell::Fish], None));
    assert_eq!(fish.choices.len(), 2);
    assert!(fish.choices[0].recommended);
    assert!(
        fish.choices[0].why.contains("function cc"),
        "{}",
        fish.choices[0].why
    );
}

#[test]
fn the_mods_part_names_the_mods_and_keeps_wiretap_a_separate_choice() {
    let dir = tempfile::tempdir().ok();
    let Some(dir) = dir else { return };
    for name in ["mike", "mods", "wiretap"] {
        let plugin = dir.path().join(name).join(".claude-plugin");
        assert!(std::fs::create_dir_all(&plugin).is_ok());
        assert!(std::fs::write(plugin.join("plugin.json"), "{}").is_ok());
    }
    let panel = mods::plan_panel(&mods::inputs_for(Some(dir.path().to_path_buf())));
    let actions: Vec<ModsAction> = panel.choices.iter().map(|c| c.action).collect();
    assert_eq!(
        actions,
        vec![
            ModsAction::Install,
            ModsAction::InstallWithWiretap,
            ModsAction::Leave
        ]
    );
    assert!(
        panel.choices[0].label.contains("mike, mods"),
        "{}",
        panel.choices[0].label
    );
    assert!(!panel.choices[0].label.contains("wiretap"));
    assert!(panel.choices[1].why.contains("every prompt and tool call"));
}

#[test]
fn the_mods_part_without_a_folder_says_so_and_only_offers_to_leave() {
    let panel = mods::plan_panel(&mods::inputs_for(None));
    assert_eq!(panel.choices.len(), 1);
    let found = panel.found.join("\n");
    assert!(found.contains("No folder of mods was found"), "{found}");
    assert!(panel.note.unwrap_or_default().contains("share/mike/mods"));
}

#[test]
fn the_trust_part_defaults_to_leaving_trust_to_claude_code() {
    let panel = trust::plan_panel(None);
    assert_eq!(panel.selected().map(|c| c.action), Some(false));
    assert!(panel.selected().is_some_and(|c| c.recommended));
    assert_eq!(panel.choices[1].commands, vec!["mike claude-code trust on"]);
    let saved = trust::plan_panel(Some(true));
    assert_eq!(saved.selected().map(|c| c.action), Some(true));
    assert!(saved.found[0].contains("marks the folder as trusted"));
}

#[tokio::test]
async fn the_route_part_saves_the_choice_and_checks_it_by_reading_it_back() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let mut step = route::RouteStep::new();
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Down), &mut harness.cx()).await;
    let outcome = step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert_eq!(outcome, Outcome::Stay);
    assert!(
        harness.config_text().contains("claude_route = \"plain\""),
        "{}",
        harness.config_text()
    );
    let text = render_step(&step);
    assert!(text.contains("Saved the route in"), "{text}");
    assert!(
        text.contains("ANTHROPIC_BASE_URL=http://127.0.0.1:2"),
        "{text}"
    );
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Next
    );
    Ok(())
}

#[tokio::test]
async fn choosing_the_sidecar_with_nothing_listening_is_a_note_not_a_failure() -> anyhow::Result<()>
{
    let mut harness = Harness::new()?;
    let mut step = route::RouteStep::new();
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert!(harness.config_text().contains("claude_route = \"sidecar\""));
    let text = render_step(&step);
    assert!(
        text.contains("◇ The sidecar does not answer on http://127.0.0.1:3"),
        "{text}"
    );
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Next
    );
    Ok(())
}

#[tokio::test]
async fn the_trust_part_writes_the_choice_to_the_config_file() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let mut step = trust::TrustStep::new();
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert!(
        harness.config_text().contains("launch_auto_trust = false"),
        "{}",
        harness.config_text()
    );
    step.key(key(KeyCode::Down), &mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert!(
        harness.config_text().contains("launch_auto_trust = true"),
        "{}",
        harness.config_text()
    );
    Ok(())
}

#[tokio::test]
async fn the_shell_part_writes_the_function_into_the_chosen_shells_file_and_renames_it()
-> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    harness.add_executable("zsh", "#!/bin/sh\n")?;
    harness.add_executable("mike", "#!/bin/sh\n")?;
    let mut step = ShellStep::new();
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    let rc = harness.home().join(".zshrc");
    let text = std::fs::read_to_string(&rc)?;
    assert!(
        text.contains("# >>> mike >>>") && text.contains("cc() {"),
        "{text}"
    );
    let shown = render_step(&step);
    assert!(shown.contains("Wrote the `cc` function for zsh"), "{shown}");
    assert!(shown.contains("Open a new terminal"), "{shown}");
    step.key(key(KeyCode::Char('r')), &mut harness.cx()).await;
    step.key(key(KeyCode::Char('e')), &mut harness.cx()).await;
    for _ in 0..2 {
        step.key(key(KeyCode::Backspace), &mut harness.cx()).await;
    }
    for event in typed("mcc") {
        step.key(event, &mut harness.cx()).await;
    }
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert!(render_step(&step).contains("mike claude-code shell --shell zsh --name mcc"));
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    let text = std::fs::read_to_string(&rc)?;
    assert_eq!(text.matches("# >>> mike >>>").count(), 1, "{text}");
    assert!(text.contains("mcc() {"), "{text}");
    assert!(!text.lines().any(|line| line == "cc() {"), "{text}");
    Ok(())
}

#[tokio::test]
async fn the_shell_part_refuses_a_name_that_is_already_a_command() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    harness.add_executable("zsh", "#!/bin/sh\n")?;
    harness.add_executable("git", "#!/bin/sh\n")?;
    let mut step = ShellStep::new();
    step.enter(&mut harness.cx()).await;
    step.key(key(KeyCode::Char('e')), &mut harness.cx()).await;
    for _ in 0..2 {
        step.key(key(KeyCode::Backspace), &mut harness.cx()).await;
    }
    for event in typed("git") {
        step.key(event, &mut harness.cx()).await;
    }
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    let text = render_step(&step);
    assert!(text.contains("already a command on PATH"), "{text}");
    assert!(!harness.home().join(".zshrc").exists());
    Ok(())
}

#[tokio::test]
async fn the_composite_walks_the_five_parts_and_back() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let mut claude = Claude::new();
    claude.enter(&mut harness.cx()).await;
    assert_eq!(claude.progress(), Some((1, 5)));
    for expected in 2..=5 {
        let outcome = claude.key(key(KeyCode::Char('s')), &mut harness.cx()).await;
        assert_eq!(outcome, Outcome::Stay);
        assert_eq!(claude.progress(), Some((expected, 5)));
    }
    assert_eq!(
        claude.key(key(KeyCode::Char('s')), &mut harness.cx()).await,
        Outcome::Next,
        "skipping the last part leaves the step"
    );
    claude.key(key(KeyCode::Left), &mut harness.cx()).await;
    assert_eq!(claude.progress(), Some((4, 5)));
    claude.enter(&mut harness.cx()).await;
    assert_eq!(
        claude.key(key(KeyCode::Left), &mut harness.cx()).await,
        Outcome::Back,
        "back from the first part leaves the step"
    );
    Ok(())
}
