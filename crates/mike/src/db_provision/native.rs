use std::fmt::Write;

use super::DbError;
use super::runner::{Call, Runner, run_checked, run_loose};
use super::up::Timing;

const BREW_FORMULA: &str = "postgresql@19";
const EXTENSIONS_SQL: &str = "CREATE EXTENSION IF NOT EXISTS vector; CREATE EXTENSION IF NOT EXISTS pgcrypto; CREATE EXTENSION IF NOT EXISTS pg_trgm;";
const NATIVE_HOST_PORT: &str = "127.0.0.1:5432";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PackageManager {
    Pacman,
    Apt,
    Dnf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Platform {
    MacWithBrew,
    MacWithoutBrew,
    Linux(Option<PackageManager>),
    Other(String),
}

pub(crate) fn detect(os: &str, on_path: &dyn Fn(&str) -> bool) -> Platform {
    match os {
        "macos" if on_path("brew") => Platform::MacWithBrew,
        "macos" => Platform::MacWithoutBrew,
        "linux" => Platform::Linux(
            [
                ("pacman", PackageManager::Pacman),
                ("apt-get", PackageManager::Apt),
                ("dnf", PackageManager::Dnf),
            ]
            .into_iter()
            .find(|(program, _)| on_path(program))
            .map(|(_, manager)| manager),
        ),
        other => Platform::Other(other.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    Brew(Vec<String>),
    BrewTool {
        tool: &'static str,
        args: Vec<String>,
    },
    WaitUntilReady,
    ManualOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Step {
    pub(crate) why: &'static str,
    pub(crate) shell: String,
    pub(crate) root: bool,
    pub(crate) action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) steps: Vec<Step>,
    pub(crate) url: String,
    pub(crate) notes: Vec<String>,
}

fn keg_tool(tool: &str) -> String {
    format!("\"$(brew --prefix {BREW_FORMULA})/bin/{tool}\"")
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(ToString::to_string).collect()
}

fn mac_steps() -> Vec<Step> {
    let tool_step = |why, tool: &'static str, args: &[&str]| Step {
        why,
        shell: format!("{} {}", keg_tool(tool), shell_args(args)),
        root: false,
        action: Action::BrewTool {
            tool,
            args: strings(args),
        },
    };
    vec![
        Step {
            why: "install PostgreSQL 19 and the pgvector extension",
            shell: format!("brew install {BREW_FORMULA} pgvector"),
            root: false,
            action: Action::Brew(strings(&["install", BREW_FORMULA, "pgvector"])),
        },
        Step {
            why: "start PostgreSQL now and at every login",
            shell: format!("brew services start {BREW_FORMULA}"),
            root: false,
            action: Action::Brew(strings(&["services", "start", BREW_FORMULA])),
        },
        Step {
            why: "wait until PostgreSQL answers",
            shell: format!("{} -h 127.0.0.1", keg_tool("pg_isready")),
            root: false,
            action: Action::WaitUntilReady,
        },
        tool_step("create the database", "createdb", &["mike"]),
        tool_step(
            "create the three extensions mike needs",
            "psql",
            &["-d", "mike", "-c", EXTENSIONS_SQL],
        ),
    ]
}

fn shell_args(args: &[&str]) -> String {
    args.iter()
        .map(|arg| {
            if arg.contains(' ') {
                format!("\"{arg}\"")
            } else {
                (*arg).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn manual(why: &'static str, shell: &str) -> Step {
    Step {
        why,
        shell: shell.to_string(),
        root: shell.starts_with("sudo"),
        action: Action::ManualOnly,
    }
}

fn linux_install_steps(manager: PackageManager) -> Vec<Step> {
    match manager {
        PackageManager::Pacman => vec![
            manual(
                "install PostgreSQL 19 and matching pgvector from a reviewed package source",
                "# Arch repositories may not provide PostgreSQL 19 yet; use mike db up when unavailable",
            ),
            manual(
                "create the cluster (skip this when /var/lib/postgres/data already holds one)",
                "sudo -iu postgres initdb --locale=C.UTF-8 --encoding=UTF8 --auth-local=peer --auth-host=scram-sha-256 -D /var/lib/postgres/data",
            ),
            manual(
                "start PostgreSQL now and at every boot",
                "sudo systemctl enable --now postgresql",
            ),
        ],
        PackageManager::Apt => vec![
            manual(
                "install PostgreSQL",
                "sudo apt install postgresql-19 postgresql-contrib-19",
            ),
            manual(
                "install pgvector for the PostgreSQL version that was installed",
                "sudo apt install postgresql-19-pgvector",
            ),
        ],
        PackageManager::Dnf => vec![
            manual(
                "install PostgreSQL and pgvector",
                "sudo dnf install postgresql19-server postgresql19-contrib pgvector_19",
            ),
            manual(
                "create the cluster",
                "sudo /usr/pgsql-19/bin/postgresql-19-setup initdb",
            ),
            manual(
                "start PostgreSQL now and at every boot",
                "sudo systemctl enable --now postgresql-19",
            ),
        ],
    }
}

fn linux_database_steps() -> Vec<Step> {
    vec![
        manual(
            "create the database user (it asks for the password to choose)",
            "sudo -u postgres createuser --pwprompt mike",
        ),
        manual(
            "create the database",
            "sudo -u postgres createdb --owner mike mike",
        ),
        manual(
            "create the three extensions mike needs, once, as the PostgreSQL superuser",
            &format!("sudo -u postgres psql -d mike -c \"{EXTENSIONS_SQL}\""),
        ),
    ]
}

pub(crate) fn plan(platform: &Platform, user: &str) -> Plan {
    match platform {
        Platform::MacWithBrew => Plan {
            steps: mac_steps(),
            url: format!("postgres://{user}@{NATIVE_HOST_PORT}/mike"),
            notes: vec!["PostgreSQL 19 is required. If Homebrew has no postgresql@19 formula, use `mike db up` to build the supported PostgreSQL 19 container; do not substitute another major.".to_owned()],
        },
        Platform::MacWithoutBrew => Plan {
            steps: Vec::new(),
            url: format!("postgres://{user}@{NATIVE_HOST_PORT}/mike"),
            notes: vec![
                "Homebrew is not installed. Install it from https://brew.sh and run `mike db native` again, or run `mike db up` with Docker, OrbStack or Colima.".to_string(),
            ],
        },
        Platform::Linux(manager) => linux_plan(*manager),
        Platform::Other(os) => Plan {
            steps: Vec::new(),
            url: format!("postgres://mike:<password>@{NATIVE_HOST_PORT}/mike"),
            notes: vec![format!(
                "mike has no install commands for {os}. Install PostgreSQL 19 with the pgvector and contrib packages, create a user and a database named mike, then run `mike db check <url>`."
            )],
        },
    }
}

fn linux_plan(manager: Option<PackageManager>) -> Plan {
    let mut steps = manager.map(linux_install_steps).unwrap_or_default();
    let mut notes =
        vec!["These steps need root, so mike only prints them and never runs sudo.".to_string()];
    notes.push("These commands require a package source that supplies PostgreSQL 19 and matching extensions. Check `postgres --version` before initializing a cluster; if 19 is unavailable, use `mike db up`.".to_owned());
    if manager.is_none() {
        notes.push(
            "No supported package manager (pacman, apt, dnf) was found. Install PostgreSQL 19 with the pgvector and contrib packages from your distribution, then continue with the steps below."
                .to_string(),
        );
    }
    steps.extend(linux_database_steps());
    Plan {
        steps,
        url: format!("postgres://mike:<password>@{NATIVE_HOST_PORT}/mike"),
        notes,
    }
}

pub(crate) fn render(plan: &Plan) -> String {
    let mut out = String::new();
    for note in &plan.notes {
        let _ = writeln!(out, "{note}\n");
    }
    for (index, step) in plan.steps.iter().enumerate() {
        let root = if step.root { " (needs root)" } else { "" };
        let _ = writeln!(
            out,
            "{}. {}{root}\n   {}\n",
            index + 1,
            step.why,
            step.shell
        );
    }
    let _ = writeln!(out, "The database URL for this setup is\n   {}", plan.url);
    let _ = writeln!(
        out,
        "\nWhen the steps are done, run `mike db check {}` to verify the version, the extensions and the migrations, and add `--save` to keep the URL in the mike config file.",
        plan.url
    );
    out
}

pub(crate) async fn execute<R: Runner>(
    runner: &R,
    plan: &Plan,
    timing: Timing,
) -> Result<(), DbError> {
    let mut prefix: Option<String> = None;
    for step in &plan.steps {
        match &step.action {
            Action::Brew(args) => {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                run_checked(runner, &Call::new("brew", &args)).await?;
            }
            Action::BrewTool { tool, args } => {
                let bin = keg_bin(runner, &mut prefix, tool).await?;
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                run_tolerating_existing(runner, &Call::new(&bin, &args)).await?;
            }
            Action::WaitUntilReady => {
                let bin = keg_bin(runner, &mut prefix, "pg_isready").await?;
                wait_until_ready(runner, &bin, timing).await?;
            }
            Action::ManualOnly => {}
        }
    }
    Ok(())
}

async fn keg_bin<R: Runner>(
    runner: &R,
    prefix: &mut Option<String>,
    tool: &str,
) -> Result<String, DbError> {
    if prefix.is_none() {
        let call = Call::new("brew", &["--prefix", BREW_FORMULA]);
        *prefix = Some(run_checked(runner, &call).await?.stdout.trim().to_string());
    }
    Ok(format!(
        "{}/bin/{tool}",
        prefix.as_deref().unwrap_or_default()
    ))
}

async fn run_tolerating_existing<R: Runner>(runner: &R, call: &Call) -> Result<(), DbError> {
    let out = run_loose(runner, call).await?;
    if out.success || out.detail().contains("already exists") {
        return Ok(());
    }
    Err(DbError::CommandFailed {
        command: call.shown(),
        detail: out.detail(),
    })
}

async fn wait_until_ready<R: Runner>(
    runner: &R,
    pg_isready: &str,
    timing: Timing,
) -> Result<(), DbError> {
    let call = Call::new(pg_isready, &["-h", "127.0.0.1"]);
    for _ in 0..timing.health_polls {
        if run_loose(runner, &call).await?.success {
            return Ok(());
        }
        tokio::time::sleep(timing.poll).await;
    }
    Err(DbError::NotReady {
        host_port: NATIVE_HOST_PORT.to_string(),
        waited_secs: timing.health_wait_secs(),
    })
}
