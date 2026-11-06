use std::time::Duration;

use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::super::facts::{Tool, ToolFact};
use super::super::panel::{Choice, Input, Panel, PanelEvent};
use super::super::report::findings_from;
use super::super::step::{Cx, Hints, Outcome, WizardStep};
use crate::config::load::redact_database_url;
use crate::db_provision::{
    Daemon, NativePlatform, SystemRunner, detect_native, docker_daemon, native_plan,
    start_brew_database,
};
use crate::doctor::database_report;
use crate::finding::Finding;

const DAEMON_WAIT: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum DbAction {
    Keep,
    Docker,
    Brew,
    Existing,
    Memory,
    Print,
}

#[derive(Debug, Clone)]
pub(in super::super) struct DbInputs {
    pub(in super::super) docker: Option<ToolFact>,
    pub(in super::super) daemon: Option<Daemon>,
    pub(in super::super) brew: Option<ToolFact>,
    pub(in super::super) platform: NativePlatform,
    pub(in super::super) configured: Option<String>,
    pub(in super::super) persist_enabled: bool,
    pub(in super::super) user: String,
}

fn found_lines(inputs: &DbInputs) -> Vec<String> {
    let mut lines = Vec::new();
    match (&inputs.docker, &inputs.daemon) {
        (Some(fact), Some(Daemon::Ready)) => {
            lines.push(format!(
                "{}; the daemon answers",
                fact.describe(Tool::Docker)
            ));
        }
        (Some(fact), Some(Daemon::Down(why))) => {
            lines.push(format!(
                "{}; the daemon does not answer ({why})",
                fact.describe(Tool::Docker)
            ));
        }
        (Some(fact), _) => lines.push(fact.describe(Tool::Docker)),
        (None, _) => {}
    }
    if let Some(fact) = &inputs.brew {
        lines.push(fact.describe(Tool::Brew));
    }
    lines.push(match (&inputs.configured, inputs.persist_enabled) {
        (Some(url), _) => format!("database_url is set: {url}"),
        (None, false) => "persist_enabled is false: captures are kept in memory only".to_string(),
        (None, true) => "No database is configured yet".to_string(),
    });
    lines
}

fn docker_choice() -> Choice<DbAction> {
    Choice::new(
        DbAction::Docker,
        "Run PostgreSQL in Docker",
        "Starts PostgreSQL 19 with pgvector in a container on 127.0.0.1, keeps the data in a Docker volume, generates a password into the mike config file and applies mike's migrations.",
    )
    .command("mike db up")
}

fn brew_choice(user: &str) -> Choice<DbAction> {
    let plan = native_plan(&NativePlatform::MacWithBrew, user);
    let mut choice = Choice::new(
        DbAction::Brew,
        "Install PostgreSQL 19 and pgvector with Homebrew",
        "Runs these Homebrew commands, which need no root, then creates the mike database and its three extensions.",
    );
    for step in plan.steps {
        choice = choice.command(step.shell);
    }
    choice
}

fn print_choice(platform: &NativePlatform, user: &str) -> Choice<DbAction> {
    let plan = native_plan(platform, user);
    let mut choice = Choice::new(
        DbAction::Print,
        "Install PostgreSQL with your package manager",
        "mike prints the commands for this machine and does not run them. Run them yourself, then choose the existing-database option with the URL they end with.",
    );
    for step in plan.steps {
        choice = if step.root {
            choice.root_command(step.shell)
        } else {
            choice.command(step.shell)
        };
    }
    choice
}

fn choices(inputs: &DbInputs) -> Vec<Choice<DbAction>> {
    let mut list = Vec::new();
    if inputs.configured.is_some() && inputs.persist_enabled {
        list.push(
            Choice::new(
                DbAction::Keep,
                "Keep the configured database",
                "Connects to it and checks the PostgreSQL version, the three extensions and the migrations.",
            )
            .recommended()
            .command("mike db check"),
        );
    }
    if inputs.docker.is_some() {
        list.push(docker_choice());
    }
    if inputs.brew.is_some() && inputs.platform == NativePlatform::MacWithBrew {
        list.push(brew_choice(&inputs.user));
    }
    if inputs.docker.is_none() && inputs.platform != NativePlatform::MacWithBrew {
        list.push(print_choice(&inputs.platform, &inputs.user));
    }
    list.push(
        Choice::new(
            DbAction::Existing,
            "Use a PostgreSQL I already run",
            "Checks the URL you type, and saves it only when the version, the extensions and the migrations pass.",
        )
        .input(Input::new("connection URL", "postgres://user:password@host:5432/mike").secret())
        .command("mike db check <url> --save"),
    );
    list.push(Choice::new(
        DbAction::Memory,
        "Keep captures in memory only",
        "Nothing is stored between runs, and the web UI shows only what happened since mike started.",
    ));
    list
}

fn recommend(list: &mut [Choice<DbAction>], inputs: &DbInputs) {
    if list.iter().any(|c| c.recommended) {
        return;
    }
    let preferred = if inputs.docker.is_some() {
        DbAction::Docker
    } else if inputs.platform == NativePlatform::MacWithBrew {
        DbAction::Brew
    } else {
        DbAction::Print
    };
    let index = list
        .iter()
        .position(|c| c.action == preferred)
        .or_else(|| list.iter().position(|c| c.action == DbAction::Existing));
    if let Some(choice) = index.and_then(|i| list.get_mut(i)) {
        choice.recommended = true;
    }
}

pub(in super::super) fn plan_panel(inputs: &DbInputs) -> Panel<DbAction> {
    let mut list = choices(inputs);
    recommend(&mut list, inputs);
    Panel::new(found_lines(inputs), list).with_note(crate::setup::plan::TRIAL_RESTART_WARNING)
}

pub(in super::super) struct Database {
    panel: Panel<DbAction>,
}

impl Database {
    pub(in super::super) fn new() -> Self {
        Self {
            panel: Panel::default(),
        }
    }

    async fn inputs(cx: &mut Cx<'_>) -> DbInputs {
        let path_var = cx.env.locations.path_var.clone();
        cx.env
            .facts
            .ensure(
                &SystemRunner,
                path_var.as_deref(),
                &[Tool::Docker, Tool::Brew],
            )
            .await;
        let docker = cx.env.facts.get(Tool::Docker).cloned();
        let daemon = match &docker {
            Some(_) => Some(docker_daemon(&SystemRunner, DAEMON_WAIT).await),
            None => None,
        };
        let on_path = |name: &str| {
            let path = path_var.as_deref();
            crate::launch::cli::locate_executable(name, path).is_some()
        };
        DbInputs {
            docker,
            daemon,
            brew: cx.env.facts.get(Tool::Brew).cloned(),
            platform: detect_native(std::env::consts::OS, &on_path),
            configured: cx.config.database_url.as_deref().map(redact_database_url),
            persist_enabled: cx.config.persist_enabled,
            user: std::env::var("USER")
                .or_else(|_| std::env::var("LOGNAME"))
                .unwrap_or_else(|_| "mike".to_string()),
        }
    }

    async fn execute(&mut self, cx: &mut Cx<'_>) {
        let Some(action) = self.panel.selected().map(|c| c.action) else {
            return;
        };
        let typed = self.panel.input_value();
        match action {
            DbAction::Print => self.panel.show(print_notes()),
            other => {
                let findings = run_action(other, &typed, cx).await;
                self.panel.record(findings);
            }
        }
    }
}

fn print_notes() -> Vec<Finding> {
    vec![Finding::note(
        "Nothing was run. After you have run the commands above, choose \"Use a PostgreSQL I already run\" and type the URL.",
    )]
}

async fn run_action(action: DbAction, typed: &str, cx: &mut Cx<'_>) -> Vec<Finding> {
    match action {
        DbAction::Keep => keep(cx).await,
        DbAction::Docker => {
            configure(
                cx,
                crate::setup::plan::StorageChoice::Docker {
                    port: 6666,
                    image: None,
                },
            )
            .await
        }
        DbAction::Brew => match start_brew_database().await {
            Ok(url) => configure(cx, crate::setup::plan::StorageChoice::Existing(url)).await,
            Err(why) => vec![Finding::failed(format!("mike db native failed: {why}"))],
        },
        DbAction::Existing => {
            configure(
                cx,
                crate::setup::plan::StorageChoice::Existing(typed.into()),
            )
            .await
        }
        DbAction::Memory => configure(cx, crate::setup::plan::StorageChoice::Trial).await,
        DbAction::Print => Vec::new(),
    }
}

async fn keep(cx: &mut Cx<'_>) -> Vec<Finding> {
    match cx.config.database_url.clone() {
        Some(url) => findings_from(&database_report(&url).await),
        None => vec![Finding::failed("No database_url is set")],
    }
}

async fn configure(cx: &mut Cx<'_>, choice: crate::setup::plan::StorageChoice) -> Vec<Finding> {
    let running = crate::stack::running_process(cx.config).is_some();
    let result = async {
        let report = crate::setup::plan::configure(cx.config, choice).await?;
        if running {
            crate::setup::plan::reconcile_selected(cx.config).await?;
        }
        anyhow::Ok(report)
    }
    .await;
    match result {
        Ok(report) => vec![Finding::ok(format!(
            "Storage saved: {} captures; selected configuration saved in {}.",
            if report.persistent {
                "persistent"
            } else {
                "in-memory"
            },
            report.config_file.display()
        ))],
        Err(why) => vec![Finding::failed(format!(
            "Storage setup incomplete: {why:#}"
        ))],
    }
}

#[async_trait(?Send)]
impl WizardStep for Database {
    fn names(&self) -> &'static [&'static str] {
        &["database"]
    }

    fn label(&self) -> &'static str {
        "database"
    }

    async fn enter(&mut self, cx: &mut Cx<'_>) {
        let inputs = Self::inputs(cx).await;
        self.panel = plan_panel(&inputs);
    }

    async fn key(&mut self, key: KeyEvent, cx: &mut Cx<'_>) -> Outcome {
        match self.panel.handle(key) {
            PanelEvent::Run => {
                self.execute(cx).await;
                Outcome::Stay
            }
            PanelEvent::Next | PanelEvent::Skip => Outcome::Next,
            PanelEvent::Back => Outcome::Back,
            PanelEvent::Recheck => {
                self.enter(cx).await;
                Outcome::Stay
            }
            PanelEvent::FinishLater => Outcome::FinishLater,
            PanelEvent::Idle => Outcome::Stay,
        }
    }

    fn draw(&self, frame: &mut Frame, area: Rect) {
        self.panel.draw(frame, area, "database");
    }

    fn hints(&self) -> Hints {
        self.panel.hints()
    }

    fn editing(&self) -> bool {
        self.panel.editing
    }
}
