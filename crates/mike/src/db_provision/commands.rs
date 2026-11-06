use anyhow::Context as _;

use super::compose::{CONTAINER, VOLUME};
use super::credentials::save_database_url;
use super::down::{DownOutcome, down};
use super::native::{self, Platform};
use super::runner::SystemRunner;
use super::server::SqlxServer;
use super::status;
use super::up::{Context, Paths, Timing, UpOutcome, port_is_free, up};
use super::{CheckArgs, DbArgs, DbCommand, NativeArgs, UpArgs};
use crate::config::CliOverrides;
use crate::doctor::{Report, database_report, render_report};

pub(super) async fn run(args: DbArgs, overrides: CliOverrides) -> i32 {
    let outcome = match args.command {
        DbCommand::Up(args) => up_command(args).await,
        DbCommand::Down => down_command().await,
        DbCommand::Status => status_command().await,
        DbCommand::Check(args) => check_command(args, overrides).await,
        DbCommand::Native(args) => native_command(args).await,
    };
    match outcome {
        Ok(code) => code,
        Err(error) => {
            eprintln!("mike db: {error:#}");
            1
        }
    }
}

fn print_progress(message: &str) {
    println!("{message}");
}

async fn up_command(args: UpArgs) -> anyhow::Result<i32> {
    let paths = Paths::from_environment()?;
    let ctx = Context {
        runner: &SystemRunner,
        server: &SqlxServer,
        paths: &paths,
        timing: Timing::REAL,
        port_free: port_is_free,
        progress: print_progress,
    };
    let outcome = up(&ctx, args.port).await?;
    print_up_outcome(&outcome, &paths);
    Ok(0)
}

fn print_up_outcome(outcome: &UpOutcome, paths: &Paths) {
    let state = if outcome.already_running {
        "was already running"
    } else {
        "is running"
    };
    println!(
        "the database {state} on 127.0.0.1:{} (container {CONTAINER}, data volume {VOLUME})",
        outcome.port
    );
    match outcome.migrations_applied {
        0 => println!("the schema is up to date"),
        applied => println!("applied {applied} migrations"),
    }
    println!(
        "saved database_url in {} (the generated password is stored there and is not printed)",
        paths.config_file.display()
    );
    if let Some(old) = &outcome.replaced_url {
        println!("it replaced the earlier database_url {old}");
    }
    if crate::user_config::overridden_by_environment("MIKE_DATABASE_URL") {
        println!(
            "MIKE_DATABASE_URL is also set in your environment or in a .env file, and that value takes precedence over the config file; remove it to use this database"
        );
    }
}

async fn down_command() -> anyhow::Result<i32> {
    let paths = Paths::from_environment()?;
    match down(&SystemRunner, &paths, Timing::REAL).await? {
        DownOutcome::NothingToStop => {
            println!(
                "`mike db up` has not set up a database on this machine, so there is nothing to stop"
            );
        }
        DownOutcome::Stopped => println!(
            "stopped the database container; the data stays in the docker volume {VOLUME}, and `mike db up` starts it again"
        ),
    }
    Ok(0)
}

async fn status_command() -> anyhow::Result<i32> {
    let paths = Paths::from_environment()?;
    let status = status::gather(&SystemRunner, &paths, Timing::REAL).await?;
    print!("{}", status::render(&status));
    Ok(0)
}

async fn check_command(args: CheckArgs, overrides: CliOverrides) -> anyhow::Result<i32> {
    let url = match args.url {
        Some(url) => url,
        None => configured_url(overrides)?,
    };
    let report = database_report(&url).await;
    print!("{}", render_report("mike db check", &report));
    if args.save {
        save_when_passing(&report, &url)?;
    }
    Ok(report.exit_code())
}

fn save_when_passing(report: &Report, url: &str) -> anyhow::Result<()> {
    if report.fail > 0 {
        println!("the URL was not saved because a check failed");
        return Ok(());
    }
    let paths = Paths::from_environment()?;
    save_database_url(&paths.config_file, url)?;
    println!("saved database_url in {}", paths.config_file.display());
    Ok(())
}

fn configured_url(overrides: CliOverrides) -> anyhow::Result<String> {
    let config =
        crate::config::load(overrides).context("the configuration could not be resolved")?;
    config.database_url.context(
        "no database URL is configured. Pass one (`mike db check postgres://user:password@host:5432/mike`) or run `mike db up`",
    )
}

async fn native_command(args: NativeArgs) -> anyhow::Result<i32> {
    let platform = native::detect(std::env::consts::OS, &crate::tui::on_path);
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .context("neither USER nor LOGNAME is set, so the database user name is unknown")?;
    let plan = native::plan(&platform, &user);
    print!("{}", native::render(&plan));
    if !args.yes {
        if platform == Platform::MacWithBrew {
            println!(
                "\nRun `mike db native --yes` to let mike run these steps itself; they need no root."
            );
        }
        return Ok(0);
    }
    if platform != Platform::MacWithBrew {
        println!(
            "\n--yes applies only to macOS with Homebrew. The steps for this machine are not run by mike."
        );
        return Ok(0);
    }
    println!("\nrunning the steps");
    native::execute(&SystemRunner, &plan, Timing::REAL).await?;
    let report = database_report(&plan.url).await;
    print!("{}", render_report("mike db check", &report));
    save_when_passing(&report, &plan.url)?;
    Ok(report.exit_code())
}
