use std::future::Future;
use std::io;
use std::sync::Mutex;

use super::{failed, lock, ok, quick};
use crate::db_provision::DbError;
use crate::db_provision::native::{
    Action, PackageManager, Platform, detect, execute, plan, render,
};
use crate::db_provision::runner::{Call, Captured, Runner};

struct Brew {
    calls: Mutex<Vec<String>>,
    isready_failures: Mutex<u32>,
    createdb_reply: &'static str,
}

impl Brew {
    fn new(isready_failures: u32, createdb_reply: &'static str) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            isready_failures: Mutex::new(isready_failures),
            createdb_reply,
        }
    }

    fn respond(&self, call: &Call) -> io::Result<Captured> {
        if call.program == "brew" && call.args.first().map(String::as_str) == Some("--prefix") {
            return ok("/opt/homebrew/opt/postgresql@19\n");
        }
        if call.program.ends_with("/pg_isready") {
            let mut left = lock(&self.isready_failures);
            if *left > 0 {
                *left -= 1;
                return failed("no response");
            }
        }
        if call.program.ends_with("/createdb") && !self.createdb_reply.is_empty() {
            return failed(self.createdb_reply);
        }
        ok("")
    }
}

impl Runner for Brew {
    fn run(&self, call: &Call) -> impl Future<Output = io::Result<Captured>> + Send {
        lock(&self.calls).push(call.shown());
        std::future::ready(self.respond(call))
    }
}

fn on(names: &'static [&'static str]) -> impl Fn(&str) -> bool {
    move |program| names.contains(&program)
}

#[test]
fn the_platform_is_chosen_from_the_os_and_the_package_manager_on_the_path() {
    assert_eq!(detect("macos", &on(&["brew"])), Platform::MacWithBrew);
    assert_eq!(detect("macos", &on(&[])), Platform::MacWithoutBrew);
    assert_eq!(
        detect("linux", &on(&["pacman", "apt-get"])),
        Platform::Linux(Some(PackageManager::Pacman))
    );
    assert_eq!(
        detect("linux", &on(&["apt-get"])),
        Platform::Linux(Some(PackageManager::Apt))
    );
    assert_eq!(
        detect("linux", &on(&["dnf"])),
        Platform::Linux(Some(PackageManager::Dnf))
    );
    assert_eq!(detect("linux", &on(&[])), Platform::Linux(None));
    assert_eq!(
        detect("freebsd", &on(&[])),
        Platform::Other("freebsd".to_string())
    );
}

#[test]
fn the_mac_plan_uses_only_commands_that_need_no_root() {
    let plan = plan(&Platform::MacWithBrew, "ada");
    assert_eq!(plan.url, "postgres://ada@127.0.0.1:5432/mike");
    assert!(
        plan.steps
            .iter()
            .all(|s| !s.root && !s.shell.contains("sudo"))
    );
    assert!(plan.steps.iter().all(|s| s.action != Action::ManualOnly));
    let shells: Vec<&str> = plan.steps.iter().map(|s| s.shell.as_str()).collect();
    assert_eq!(shells[0], "brew install postgresql@19 pgvector");
    assert_eq!(shells[1], "brew services start postgresql@19");
    assert!(shells[3].ends_with("/bin/createdb\" mike"), "{shells:?}");
    assert!(
        shells[4].contains("CREATE EXTENSION IF NOT EXISTS vector"),
        "{shells:?}"
    );
}

#[test]
fn linux_plans_are_printed_only_and_every_privileged_step_says_so() {
    for manager in [
        PackageManager::Pacman,
        PackageManager::Apt,
        PackageManager::Dnf,
    ] {
        let plan = plan(&Platform::Linux(Some(manager)), "ada");
        assert!(
            plan.steps.iter().all(|s| s.action == Action::ManualOnly),
            "{manager:?}"
        );
        assert!(
            plan.steps
                .iter()
                .all(|s| s.root == s.shell.starts_with("sudo"))
        );
        assert!(
            plan.steps.iter().any(|s| s.shell.contains("createuser")),
            "{manager:?}"
        );
        let text = render(&plan);
        assert!(text.contains("never runs sudo"), "{text}");
        assert!(text.contains("(needs root)"), "{text}");
        assert!(
            text.contains("mike db check postgres://mike:<password>@127.0.0.1:5432/mike"),
            "{text}"
        );
    }
    let pacman = plan(&Platform::Linux(Some(PackageManager::Pacman)), "ada");
    assert!(
        pacman.steps[0]
            .shell
            .contains("use mike db up when unavailable")
    );
    assert!(pacman.steps[1].shell.contains("initdb"));
    let apt = plan(&Platform::Linux(Some(PackageManager::Apt)), "ada");
    assert!(apt.steps[1].shell.contains("-pgvector"));
}

#[test]
fn a_linux_without_a_known_package_manager_still_gets_the_database_steps() {
    let plan = plan(&Platform::Linux(None), "ada");
    assert!(
        plan.notes
            .iter()
            .any(|n| n.contains("No supported package manager"))
    );
    assert!(plan.steps.iter().any(|s| s.shell.contains("createdb")));
}

#[test]
fn a_mac_without_homebrew_is_pointed_at_homebrew_or_docker() {
    let plan = plan(&Platform::MacWithoutBrew, "ada");
    assert!(plan.steps.is_empty());
    assert!(plan.notes[0].contains("brew.sh") && plan.notes[0].contains("mike db up"));
}

#[tokio::test]
async fn executing_the_mac_plan_runs_brew_then_the_keg_tools_in_order() -> anyhow::Result<()> {
    let brew = Brew::new(1, "");
    execute(&brew, &plan(&Platform::MacWithBrew, "ada"), quick()).await?;
    let calls = lock(&brew.calls).clone();
    let tools = "/opt/homebrew/opt/postgresql@19/bin";
    assert_eq!(calls[0], "brew install postgresql@19 pgvector");
    assert_eq!(calls[1], "brew services start postgresql@19");
    assert_eq!(calls[2], "brew --prefix postgresql@19");
    assert_eq!(calls[3], format!("{tools}/pg_isready -h 127.0.0.1"));
    assert_eq!(calls[4], format!("{tools}/pg_isready -h 127.0.0.1"));
    assert_eq!(calls[5], format!("{tools}/createdb mike"));
    assert!(
        calls[6].starts_with(&format!("{tools}/psql -d mike -c")),
        "{calls:?}"
    );
    assert!(calls.iter().all(|c| !c.contains("sudo")));
    Ok(())
}

#[tokio::test]
async fn an_existing_database_does_not_stop_the_mac_plan() -> anyhow::Result<()> {
    let brew = Brew::new(
        0,
        "createdb: error: database creation failed: database \"mike\" already exists",
    );
    execute(&brew, &plan(&Platform::MacWithBrew, "ada"), quick()).await?;
    assert!(
        lock(&brew.calls)
            .last()
            .is_some_and(|c| c.contains("/psql"))
    );
    Ok(())
}

#[tokio::test]
async fn a_postgres_that_never_answers_fails_the_mac_plan() -> anyhow::Result<()> {
    let brew = Brew::new(100, "");
    let result = execute(&brew, &plan(&Platform::MacWithBrew, "ada"), quick()).await;
    assert!(
        matches!(result, Err(DbError::NotReady { .. })),
        "{result:?}"
    );
    assert!(lock(&brew.calls).iter().all(|c| !c.contains("createdb")));
    Ok(())
}

#[tokio::test]
async fn a_failing_brew_install_stops_the_plan_with_brews_message() -> anyhow::Result<()> {
    struct Broken;
    impl Runner for Broken {
        fn run(&self, call: &Call) -> impl Future<Output = io::Result<Captured>> + Send {
            std::future::ready(if call.program == "brew" {
                failed("Error: No formulae found")
            } else {
                ok("")
            })
        }
    }
    let result = execute(&Broken, &plan(&Platform::MacWithBrew, "ada"), quick()).await;
    let Err(DbError::CommandFailed { command, detail }) = result else {
        anyhow::bail!("expected CommandFailed, got {result:?}");
    };
    assert_eq!(command, "brew install postgresql@19 pgvector");
    assert!(detail.contains("No formulae found"), "{detail}");
    Ok(())
}

#[tokio::test]
async fn executing_a_printed_only_plan_runs_nothing() -> anyhow::Result<()> {
    let brew = Brew::new(0, "");
    execute(
        &brew,
        &plan(&Platform::Linux(Some(PackageManager::Pacman)), "ada"),
        quick(),
    )
    .await?;
    assert!(lock(&brew.calls).is_empty());
    Ok(())
}
