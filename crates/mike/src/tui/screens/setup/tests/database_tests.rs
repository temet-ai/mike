use crossterm::event::KeyCode;

use super::super::facts::ToolFact;
use super::super::harness::{Harness, key, render_step, typed};
use super::super::step::{Outcome, WizardStep};
use super::super::steps::database::{Database, DbAction, DbInputs, plan_panel};
use crate::db_provision::{Daemon, NativePlatform};

fn fact(path: &str) -> Option<ToolFact> {
    Some(ToolFact {
        path: path.into(),
        version: Some("1.2.3".to_string()),
    })
}

fn inputs() -> DbInputs {
    DbInputs {
        docker: None,
        daemon: None,
        brew: None,
        platform: NativePlatform::Other("plan9".to_string()),
        configured: None,
        persist_enabled: true,
        user: "ada".to_string(),
    }
}

fn actions(inputs: &DbInputs) -> Vec<DbAction> {
    plan_panel(inputs)
        .choices
        .iter()
        .map(|c| c.action)
        .collect()
}

fn recommended(inputs: &DbInputs) -> Vec<DbAction> {
    plan_panel(inputs)
        .choices
        .iter()
        .filter(|c| c.recommended)
        .map(|c| c.action)
        .collect()
}

#[test]
fn with_docker_the_recommendation_is_docker_and_only_one_choice_is_recommended() {
    let mut with_docker = inputs();
    with_docker.docker = fact("/usr/local/bin/docker");
    with_docker.daemon = Some(Daemon::Ready);
    assert_eq!(recommended(&with_docker), vec![DbAction::Docker]);
    let list = actions(&with_docker);
    assert!(list.contains(&DbAction::Existing) && list.contains(&DbAction::Memory));
    assert!(
        !list.contains(&DbAction::Brew),
        "brew is not on this machine"
    );
    let panel = plan_panel(&with_docker);
    assert_eq!(panel.selected().map(|c| c.action), Some(DbAction::Docker));
    assert_eq!(
        panel.selected().map(|c| c.commands.clone()),
        Some(vec!["mike db up".to_string()])
    );
}

#[test]
fn with_homebrew_on_a_mac_the_recommendation_is_the_homebrew_install_and_the_commands_are_listed() {
    let mut mac = inputs();
    mac.brew = fact("/opt/homebrew/bin/brew");
    mac.platform = NativePlatform::MacWithBrew;
    assert_eq!(recommended(&mac), vec![DbAction::Brew]);
    let panel = plan_panel(&mac);
    let brew = panel
        .choices
        .iter()
        .find(|c| c.action == DbAction::Brew)
        .map(|c| c.commands.clone())
        .unwrap_or_default();
    assert!(
        brew.iter()
            .any(|c| c.contains("brew install postgresql@19 pgvector")),
        "{brew:?}"
    );
    assert!(brew.iter().all(|c| !c.contains("sudo")));
}

#[test]
fn without_docker_or_brew_the_package_manager_commands_are_printed_not_run() {
    let mut linux = inputs();
    linux.platform = NativePlatform::Linux(None);
    assert_eq!(recommended(&linux), vec![DbAction::Print]);
    let panel = plan_panel(&linux);
    let print = panel
        .choices
        .iter()
        .find(|c| c.action == DbAction::Print)
        .map(|c| (c.commands.clone(), c.root_commands.clone()))
        .unwrap_or_default();
    assert!(print.1.iter().all(|c| c.starts_with("sudo")), "{print:?}");
    assert!(!print.1.is_empty());
}

#[test]
fn a_configured_database_is_kept_by_default_and_a_memory_only_install_says_so() {
    let mut configured = inputs();
    configured.configured = Some("postgres://mike:***@127.0.0.1:6666/mike".to_string());
    configured.docker = fact("/usr/local/bin/docker");
    assert_eq!(recommended(&configured), vec![DbAction::Keep]);
    let found = plan_panel(&configured).found.join("\n");
    assert!(
        found.contains("database_url is set: postgres://mike:***@"),
        "{found}"
    );
    let mut memory = inputs();
    memory.persist_enabled = false;
    let found = plan_panel(&memory).found.join("\n");
    assert!(found.contains("memory only"), "{found}");
    assert!(!actions(&memory).contains(&DbAction::Keep));
}

#[test]
fn a_docker_that_does_not_answer_is_said_so_in_the_found_lines() {
    let mut down = inputs();
    down.docker = fact("/usr/local/bin/docker");
    down.daemon = Some(Daemon::Down("Cannot connect".to_string()));
    let found = plan_panel(&down).found.join("\n");
    assert!(
        found.contains("the daemon does not answer (Cannot connect)"),
        "{found}"
    );
}

#[tokio::test]
async fn choosing_memory_only_writes_the_flag_and_updates_the_running_config() -> anyhow::Result<()>
{
    let mut harness = Harness::new()?;
    let mut step = Database::new();
    step.enter(&mut harness.cx()).await;
    loop {
        let text = render_step(&step);
        if text.contains("▌ Keep captures in memory only") {
            break;
        }
        step.key(key(KeyCode::Down), &mut harness.cx()).await;
    }
    harness.config.persist_enabled = true;
    let outcome = step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert_eq!(outcome, Outcome::Stay);
    assert!(!harness.config.persist_enabled);
    assert!(
        harness.config_text().contains("persist_enabled = false"),
        "{}",
        harness.config_text()
    );
    let text = render_step(&step);
    assert!(text.contains("Storage saved:"), "{text}");
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Next
    );
    Ok(())
}

#[tokio::test]
async fn a_database_url_that_fails_its_check_is_not_saved() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let mut step = Database::new();
    step.enter(&mut harness.cx()).await;
    while !render_step(&step).contains("▌ Use a PostgreSQL I already run") {
        step.key(key(KeyCode::Down), &mut harness.cx()).await;
    }
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    for event in typed("postgres://nobody:pw@127.0.0.1:1/mike") {
        step.key(event, &mut harness.cx()).await;
    }
    assert!(step.editing());
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert!(!step.editing());
    let outcome = step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert_eq!(outcome, Outcome::Stay);
    let text = render_step(&step);
    assert!(text.contains("Storage setup incomplete"), "{text}");
    assert!(
        !harness.config_text().contains("database_url"),
        "{}",
        harness.config_text()
    );
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Stay,
        "a failed check never continues"
    );
    Ok(())
}
