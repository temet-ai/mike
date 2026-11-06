use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use crossterm::event::KeyCode;
use uuid::Uuid;

use super::Terminal;
use super::plan::{BackendSource, TermAction, TermInputs, installed, ordered, plan_panel};
use crate::launch::{
    BackendId, Capabilities, Check, CloseOutcome, LaunchError, OpenRequest, ProbeReport, Surface,
    TerminalBackend, ValidationReport, backend_for,
};
use crate::tui::screens::setup::facts::ToolFact;
use crate::tui::screens::setup::harness::{Harness, key, render_step, words};
use crate::tui::screens::setup::step::{Outcome, WizardStep};

static PROBES: AtomicUsize = AtomicUsize::new(0);

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Scripted {
    probe_ok: bool,
    validate_ok: bool,
}

#[async_trait]
impl TerminalBackend for Scripted {
    fn id(&self) -> BackendId {
        BackendId::Herdr
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            placement_workspace: true,
            placement_tab: true,
            placement_split: true,
            agent_state: true,
            prompt_in_argv: true,
            close_by_session: true,
            events: false,
        }
    }

    async fn probe(&self) -> ProbeReport {
        PROBES.fetch_add(1, Ordering::SeqCst);
        let check = if self.probe_ok {
            Check::pass("herdr server answers", "scripted answer")
        } else {
            Check::fail(
                "herdr server answers",
                "scripted silence",
                "start herdr so its server socket answers",
            )
        };
        ProbeReport {
            backend: BackendId::Herdr,
            available: self.probe_ok,
            version: Some("0.9.0".into()),
            protocol: Some("22".into()),
            checks: vec![check],
        }
    }

    async fn validate(&self) -> ValidationReport {
        let checks = vec![
            Check::pass("workspace create", "workspace w1, root pane p1"),
            if self.validate_ok {
                Check::pass("pane run", "ran the marker command in p1")
            } else {
                Check::fail(
                    "pane run",
                    "pane refused input",
                    "check the pane accepts input (`herdr pane read`)",
                )
            },
            Check::pass("workspace close", "closed w1"),
        ];
        ValidationReport::from_checks(BackendId::Herdr, checks, Utc::now())
    }

    async fn open(&self, _req: &OpenRequest) -> Result<Surface, LaunchError> {
        Err(LaunchError::Failed("scripted".into()))
    }

    async fn find_by_session(&self, _session: Uuid) -> Result<Option<Surface>, LaunchError> {
        Ok(None)
    }

    async fn close(
        &self,
        _surface: &Surface,
        _wait: Duration,
    ) -> Result<CloseOutcome, LaunchError> {
        Ok(CloseOutcome::NotFound)
    }

    async fn list(&self) -> Result<Vec<Surface>, LaunchError> {
        Ok(Vec::new())
    }
}

fn scripted(id: BackendId, probe_ok: bool, validate_ok: bool) -> Option<Box<dyn TerminalBackend>> {
    match id {
        BackendId::Herdr => Some(Box::new(Scripted {
            probe_ok,
            validate_ok,
        })),
        BackendId::None => backend_for(BackendId::None),
        BackendId::HerdrCore | BackendId::Tmux | BackendId::Terminal | BackendId::Ghostty => None,
    }
}

fn herdr_ready(id: BackendId) -> Option<Box<dyn TerminalBackend>> {
    scripted(id, true, true)
}

fn herdr_validation_fails(id: BackendId) -> Option<Box<dyn TerminalBackend>> {
    scripted(id, true, false)
}

fn herdr_not_found(id: BackendId) -> Option<Box<dyn TerminalBackend>> {
    scripted(id, false, false)
}

fn fact(path: &str) -> Option<ToolFact> {
    Some(ToolFact {
        path: path.into(),
        version: Some("1.0".to_string()),
    })
}

fn inputs() -> TermInputs {
    TermInputs {
        herdr: None,
        tmux: None,
        hyprctl: false,
        xdg: false,
        brew: false,
        current: None,
        probed: Vec::new(),
    }
}

fn ids(inputs: &TermInputs) -> Vec<BackendId> {
    ordered(inputs)
}

#[test]
fn only_backends_whose_tools_are_here_are_listed_and_none_is_always_there() {
    assert_eq!(installed(&inputs()), vec![BackendId::None]);
    let mut mac = inputs();
    mac.herdr = fact("/opt/homebrew/bin/herdr");
    mac.tmux = fact("/opt/homebrew/bin/tmux");
    assert_eq!(
        installed(&mac),
        vec![BackendId::Herdr, BackendId::Tmux, BackendId::None]
    );
    assert!(!installed(&mac).contains(&BackendId::Ghostty));
}

#[test]
fn the_hyprland_terminal_needs_both_hyprctl_and_xdg_terminal_exec() {
    let mut only_hyprctl = inputs();
    only_hyprctl.hyprctl = true;
    assert!(!installed(&only_hyprctl).contains(&BackendId::Terminal));
    let mut only_xdg = inputs();
    only_xdg.xdg = true;
    assert!(!installed(&only_xdg).contains(&BackendId::Terminal));
    let mut both = inputs();
    both.hyprctl = true;
    both.xdg = true;
    assert_eq!(installed(&both), vec![BackendId::Terminal, BackendId::None]);
}

#[test]
fn the_current_backend_comes_first_and_is_marked() {
    let mut machine = inputs();
    machine.herdr = fact("/bin/herdr");
    machine.tmux = fact("/bin/tmux");
    machine.current = Some(BackendId::Tmux);
    assert_eq!(
        ids(&machine),
        vec![BackendId::Tmux, BackendId::Herdr, BackendId::None]
    );
    let panel = plan_panel(&machine);
    assert_eq!(panel.choices[0].label, "tmux (current)");
    assert_eq!(
        panel.selected().map(|c| c.action),
        Some(TermAction::Backend(BackendId::Herdr))
    );
    assert_eq!(panel.choices.iter().filter(|c| c.recommended).count(), 1);
}

#[test]
fn herdr_is_recommended_when_installed_and_offered_for_install_when_not() {
    let mut with = inputs();
    with.herdr = fact("/bin/herdr");
    assert!(
        plan_panel(&with)
            .selected()
            .is_some_and(|c| c.action == TermAction::Backend(BackendId::Herdr))
    );
    let mut brew = inputs();
    brew.brew = true;
    let panel = plan_panel(&brew);
    assert_eq!(panel.choices[0].action, TermAction::InstallHerdr);
    assert_eq!(panel.choices[0].commands, vec!["brew install herdr"]);
    assert!(panel.choices[0].recommended);
    let without = plan_panel(&inputs());
    assert_eq!(without.choices[0].action, TermAction::InstallHerdr);
    assert!(without.choices[0].commands.is_empty());
    assert!(without.choices[0].why.contains("https://herdr.dev"));
}

#[test]
fn each_backend_shows_the_live_check_command_with_its_name() {
    let mut machine = inputs();
    machine.herdr = fact("/bin/herdr");
    let panel = plan_panel(&machine);
    let herdr = panel
        .choices
        .iter()
        .find(|c| c.action == TermAction::Backend(BackendId::Herdr))
        .map(|c| c.commands.clone());
    assert_eq!(
        herdr,
        Some(vec![
            "MIKE_LAUNCH_BACKEND=herdr mike spawn doctor".to_string()
        ])
    );
}

async fn at_step(backends: BackendSource, tools: &[&str]) -> anyhow::Result<(Terminal, Harness)> {
    let mut harness = Harness::new()?;
    for tool in tools {
        harness.add_executable(tool, "#!/bin/sh\necho 1.0\n")?;
    }
    let mut step = Terminal::with_source(backends);
    step.enter(&mut harness.cx()).await;
    Ok((step, harness))
}

#[tokio::test]
async fn the_step_lists_the_installed_backends_with_what_their_probe_said() -> anyhow::Result<()> {
    let (step, _harness) = at_step(herdr_ready, &["herdr", "tmux"]).await?;
    let text = render_step(&step);
    assert!(text.contains("herdr is ready"), "{text}");
    assert!(text.contains("herdr (recommended)"), "{text}");
    assert!(text.contains("tmux"), "{text}");
    assert!(text.contains("none"), "{text}");
    assert!(!text.contains("ghostty"), "{text}");
    assert!(!text.contains("not in this build"), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_probe_that_fails_shows_its_first_remedy() -> anyhow::Result<()> {
    let (step, _harness) = at_step(herdr_not_found, &["herdr"]).await?;
    let text = render_step(&step);
    assert!(
        text.contains("herdr is not ready: start herdr so its server socket answers"),
        "{text}"
    );
    Ok(())
}

#[tokio::test]
async fn the_step_uses_one_status_vocabulary() -> anyhow::Result<()> {
    let (step, _harness) = at_step(herdr_not_found, &["herdr"]).await?;
    let vocabulary = words(&render_step(&step));
    for banned in [
        "up",
        "down",
        "ok",
        "active",
        "inert",
        "healthy",
        "available",
        "online",
        "offline",
        "missing",
        "present",
        "pass",
        "passed",
    ] {
        assert!(!vocabulary.contains(banned), "banned word {banned:?}");
    }
    Ok(())
}

#[tokio::test]
async fn a_passed_check_saves_the_backend_and_continues() -> anyhow::Result<()> {
    let _env = ENV_LOCK.lock().await;
    let (mut step, mut harness) = at_step(herdr_ready, &["herdr"]).await?;
    let outcome = step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert_eq!(outcome, Outcome::Stay);
    assert_eq!(harness.config_text(), "launch_backend = \"herdr\"\n");
    assert_eq!(harness.config.launch.backend, "herdr");
    assert_eq!(
        std::env::var("MIKE_LAUNCH_BACKEND").ok().as_deref(),
        Some("herdr")
    );
    let text = render_step(&step);
    assert!(
        text.contains("workspace create: workspace w1, root pane p1"),
        "{text}"
    );
    assert!(text.contains("Saved launch_backend = herdr"), "{text}");
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Next
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_check_saves_nothing_and_shows_the_failing_check_with_its_fix()
-> anyhow::Result<()> {
    let (mut step, mut harness) = at_step(herdr_validation_fails, &["herdr"]).await?;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert_eq!(harness.config_text(), "");
    let text = render_step(&step);
    assert!(
        text.contains(
            "pane run: pane refused input · fix: check the pane accepts input (`herdr pane read`)"
        ),
        "{text}"
    );
    assert!(text.contains("so nothing was saved"), "{text}");
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Stay
    );
    Ok(())
}

#[tokio::test]
async fn a_backend_that_does_not_answer_its_probe_is_not_validated_or_saved() -> anyhow::Result<()>
{
    let (mut step, mut harness) = at_step(herdr_not_found, &["herdr"]).await?;
    let before = PROBES.load(Ordering::SeqCst);
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert!(PROBES.load(Ordering::SeqCst) > before);
    assert_eq!(harness.config_text(), "");
    let text = render_step(&step);
    assert!(
        text.contains("herdr server answers: scripted silence · fix: start herdr"),
        "{text}"
    );
    Ok(())
}

#[tokio::test]
async fn none_is_saved_without_any_live_check() -> anyhow::Result<()> {
    let _env = ENV_LOCK.lock().await;
    let (mut step, mut harness) = at_step(herdr_not_found, &[]).await?;
    let none_at = step
        .panel
        .choices
        .iter()
        .position(|c| c.action == TermAction::Backend(BackendId::None));
    step.panel.cursor = none_at.unwrap_or(0);
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    assert_eq!(harness.config_text(), "launch_backend = \"none\"\n");
    assert_eq!(
        std::env::var("MIKE_LAUNCH_BACKEND").ok().as_deref(),
        Some("none")
    );
    Ok(())
}

#[tokio::test]
async fn an_unwritable_config_file_is_reported_and_the_step_does_not_continue() -> anyhow::Result<()>
{
    let (mut step, mut harness) = at_step(herdr_ready, &["herdr"]).await?;
    std::fs::create_dir_all(harness.env.config_file())?;
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    let text = render_step(&step);
    assert!(text.contains("Could not write"), "{text}");
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Stay
    );
    Ok(())
}

#[tokio::test]
async fn choosing_to_install_herdr_without_homebrew_only_names_where_to_get_it()
-> anyhow::Result<()> {
    let (mut step, mut harness) = at_step(herdr_ready, &[]).await?;
    assert_eq!(
        step.panel.selected().map(|c| c.action),
        Some(TermAction::InstallHerdr)
    );
    step.key(key(KeyCode::Enter), &mut harness.cx()).await;
    let text = render_step(&step);
    assert!(
        text.contains("Nothing was run. Install herdr from https://herdr.dev"),
        "{text}"
    );
    assert_eq!(
        step.key(key(KeyCode::Enter), &mut harness.cx()).await,
        Outcome::Stay
    );
    Ok(())
}

#[tokio::test]
async fn skip_and_back_and_finish_later_leave_the_step_without_writing() -> anyhow::Result<()> {
    let (mut step, mut harness) = at_step(herdr_ready, &["herdr"]).await?;
    assert_eq!(
        step.key(key(KeyCode::Char('s')), &mut harness.cx()).await,
        Outcome::Next
    );
    assert_eq!(
        step.key(key(KeyCode::Left), &mut harness.cx()).await,
        Outcome::Back
    );
    assert_eq!(
        step.key(key(KeyCode::Char('f')), &mut harness.cx()).await,
        Outcome::FinishLater
    );
    assert_eq!(harness.config_text(), "");
    Ok(())
}

#[test]
fn the_sandbox_source_only_builds_the_none_backend() {
    use super::plan::sandbox_backend_for;
    assert!(sandbox_backend_for(BackendId::None).is_some());
    for id in [
        BackendId::Herdr,
        BackendId::Tmux,
        BackendId::Terminal,
        BackendId::Ghostty,
    ] {
        assert!(sandbox_backend_for(id).is_none(), "{id}");
    }
}

#[tokio::test]
async fn live_terminal_step_renders_the_real_herdr_when_opted_in() -> anyhow::Result<()> {
    if std::env::var("MIKE_LIVE_HERDR").as_deref() != Ok("1") {
        return Ok(());
    }
    let _env = ENV_LOCK.lock().await;
    let (step, _harness) = at_step(backend_for, &["herdr"]).await?;
    println!("--- probed ---\n{}", render_step(&step));
    Ok(())
}
