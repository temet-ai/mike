pub mod api;
pub mod check;
pub mod commit;
pub mod gate;
pub mod record;
#[cfg(test)]
pub mod tests;

use std::io::IsTerminal;
use std::time::Duration;

use clap::{Args, Subcommand};

pub use self::api::{Api, FleetError};
use super::render::{DEFAULT_WIDTH, FleetListing, render, render_list, strip};
use super::shutdown::StopReport;
use super::status::FleetStatus;
use crate::config::CliOverrides;

pub const LIST_LIMIT: &str = "50";
const CLEAR_SCREEN: &str = "\x1b[H\x1b[2J";

#[derive(Debug, Clone, Args)]
pub struct FleetArgs {
    #[command(subcommand)]
    pub command: FleetCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum FleetCommand {
    #[command(about = "Draw where one fleet stands: tasks, seats, verdicts and what is blocked")]
    Status(StatusArgs),
    #[command(about = "List the fleets defined in this mike, newest first")]
    List(ListArgs),
    #[command(about = "Start a fleet that is ready but waiting for your go-ahead")]
    Start(StartArgs),
    #[command(
        about = "Request fleet shutdown and report any seats or managed commands still settling"
    )]
    Stop(StartArgs),
    #[command(about = "One line for a status bar: the newest running fleet's progress")]
    Summary,
    #[command(
        about = "Bring a seat's pane to the front: the named seat, else the first blocked seat, else the seat of the newest FAIL"
    )]
    Focus(FocusArgs),
    #[command(
        about = "Commit a seat's changed files through mike and push: its task's files, or the paths named, without touching the shared index"
    )]
    Commit(commit::CommitArgs),
    #[command(
        about = "Check the fleet's checkout with the project's declared check command, run by mike once per state of the tree and shared by every seat"
    )]
    Check(check::CheckArgs),
    #[command(about = "Where a name is defined in the fleet's checkout, in any language")]
    Where(check::WhereArgs),
    #[command(
        about = "Run the fleet's full gate (gate.full_command, else gate.command) at the branch's newest commit, in mike's own checkout beside the repository, once per commit; for the acceptance seat"
    )]
    Gate(gate::GateArgs),
    #[command(
        about = "Print a finished run's record: time to done, minutes lost to faults, tokens per accepted task and manual interventions, or the difference from another run"
    )]
    Record(record::RecordArgs),
}

#[derive(Debug, Clone, Args)]
pub struct FocusArgs {
    #[arg(help = "The fleet's uuid or its slug")]
    pub fleet: String,
    #[arg(help = "The seat to bring to the front")]
    pub seat: Option<String>,
    #[arg(
        long,
        help = "Switch to the fleet's workspace even when another one is in front"
    )]
    pub switch: bool,
}

#[derive(Debug, Clone, Args)]
pub struct StatusArgs {
    #[arg(help = "The fleet's uuid or its slug")]
    pub fleet: String,
    #[arg(
        long,
        value_name = "SECONDS",
        value_parser = clap::value_parser!(u64).range(1..),
        help = "Redraw every SECONDS seconds until interrupted (at least 1)"
    )]
    pub watch: Option<u64>,
    #[arg(long, help = "Print the status as JSON, exactly as the API answered")]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ListArgs {
    #[arg(
        long,
        value_name = "PATH",
        help = "Only the fleets of this project path"
    )]
    pub project: Option<String>,
    #[arg(long, help = "Print the list as JSON, exactly as the API answered")]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct StartArgs {
    #[arg(help = "The fleet's uuid or its slug")]
    pub fleet: String,
}

#[derive(serde::Deserialize)]
pub struct ListAnswer {
    pub fleets: Vec<FleetListing>,
}

#[derive(serde::Deserialize)]
struct StartAnswer {
    slug: Option<String>,
    fleet: Option<String>,
    state: String,
    run_id: Option<String>,
}

pub async fn run(args: FleetArgs, overrides: CliOverrides) -> i32 {
    let config = match crate::config::load(overrides) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("mike fleet: failed to load configuration: {error:#}");
            return 1;
        }
    };
    let api = Api::new(&config.inspect_listen_addr);
    match args.command {
        FleetCommand::Status(status) => match status.watch {
            Some(seconds) => watch(&api, &status, seconds).await,
            None => finish(status_text(&api, &status).await),
        },
        FleetCommand::List(list) => finish(list_text(&api, &list).await),
        FleetCommand::Start(start) => finish(start_text(&api, &start).await),
        FleetCommand::Stop(stop) => finish(stop_text(&api, &stop).await),
        FleetCommand::Summary => finish(summary_text(&api).await),
        FleetCommand::Focus(focus) => finish(super::focus::run(&api, &focus).await),
        other => finish(seat_command_text(&api, other).await),
    }
}

async fn seat_command_text(api: &Api, command: FleetCommand) -> Result<String, FleetError> {
    match command {
        FleetCommand::Commit(args) => commit::commit_text(api, &args).await,
        FleetCommand::Check(args) => check::check_text(api, &args).await,
        FleetCommand::Where(args) => check::where_text(api, &args).await,
        FleetCommand::Gate(args) => gate::gate_text(api, &args).await,
        FleetCommand::Record(args) => record::record_text(api, &args).await,
        _ => Err(FleetError::Refused(
            "this command is handled elsewhere".to_string(),
        )),
    }
}

fn finish(result: Result<String, FleetError>) -> i32 {
    match result {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(error) => {
            eprintln!("mike fleet: {error}");
            1
        }
    }
}

async fn watch(api: &Api, args: &StatusArgs, seconds: u64) -> i32 {
    loop {
        let text = match status_text(api, args).await {
            Ok(text) => text,
            Err(error) => format!("mike fleet: {error}"),
        };
        println!("{CLEAR_SCREEN}{text}");
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(seconds)) => {}
            _ = tokio::signal::ctrl_c() => return 0,
        }
    }
}

async fn status_text(api: &Api, args: &StatusArgs) -> Result<String, FleetError> {
    let body = api.get(api.url(&[&args.fleet, "status"])?).await?;
    if args.json {
        return Ok(body.trim_end().to_string());
    }
    let status: FleetStatus = api.parse(&body)?;
    Ok(for_terminal(render(&status, terminal_width())))
}

async fn list_text(api: &Api, args: &ListArgs) -> Result<String, FleetError> {
    let mut url = api.url(&[])?;
    {
        let mut query = url.query_pairs_mut();
        if let Some(project) = &args.project {
            query.append_pair("project_path", project);
        }
        query.append_pair("limit", LIST_LIMIT);
    }
    let body = api.get(url).await?;
    if args.json {
        return Ok(body.trim_end().to_string());
    }
    let answer: ListAnswer = api.parse(&body)?;
    Ok(for_terminal(render_list(&answer.fleets, terminal_width())))
}

async fn start_text(api: &Api, args: &StartArgs) -> Result<String, FleetError> {
    let body = api.post(api.url(&[&args.fleet, "start"])?).await?;
    let answer: StartAnswer = api.parse(&body)?;
    let name = answer
        .slug
        .or(answer.fleet)
        .unwrap_or_else(|| args.fleet.clone());
    Ok(match answer.run_id {
        Some(run) => format!("{name}: {} (run {run})", answer.state),
        None => format!("{name}: {}", answer.state),
    })
}

async fn stop_text(api: &Api, args: &StartArgs) -> Result<String, FleetError> {
    let body = api.post(api.url(&[&args.fleet, "stop"])?).await?;
    let report: StopReport = api.parse(&body)?;
    Ok(render_stop(&report))
}

fn render_stop(report: &StopReport) -> String {
    let mut lines = vec![
        format!("{}: {}", report.slug, report.state),
        format!("  managed containment: {}", report.managed.containment),
    ];
    if !report.root_origin.is_empty() {
        lines.push(format!("  seat root origin: {}", report.root_origin));
    }
    for seat in &report.seats {
        let name = seat.seat.as_deref().unwrap_or("seat");
        let pane = if seat.closed {
            "pane close acknowledged"
        } else {
            "pane close unconfirmed"
        };
        let root = match seat.process_exited {
            Some(true) => "root process exited",
            Some(false) => "root process still present",
            None => "root process exit unknown",
        };
        let outcome = format!("{pane}; {root}");
        match &seat.note {
            Some(note) => lines.push(format!("  {name}: {outcome} ({note})")),
            None => lines.push(format!("  {name}: {outcome}")),
        }
    }
    for owner in &report.managed.pending_owners {
        lines.push(format!("  managed command still settling: {owner}"));
    }
    if !report.managed.pending_receipts.is_empty() {
        lines.push(format!(
            "  unfinished command receipts: {:?}",
            report.managed.pending_receipts
        ));
    }
    if report.state == super::shutdown::STOPPING {
        lines.push("  The stop fence is active. Retry fleet stop to reconcile; inspect unresolved launch/command receipts before retrying their work. Keep the checkout until shutdown settles.".into());
    }
    if !report.untracked_descendants.is_empty() {
        lines.push(format!("  {}", report.untracked_descendants));
    }
    lines.join("\n")
}

async fn summary_text(api: &Api) -> Result<String, FleetError> {
    let mut url = api.url(&[])?;
    url.query_pairs_mut().append_pair("limit", LIST_LIMIT);
    let body = api.get(url).await?;
    let answer: ListAnswer = api.parse(&body)?;
    let Some(running) = answer.fleets.iter().find(|f| f.state == "running") else {
        return Ok(String::new());
    };
    let body = api.get(api.url(&[&running.fleet, "status"])?).await?;
    let status: FleetStatus = api.parse(&body)?;
    Ok(format!(
        "{} {}",
        running.slug,
        super::display::progress_line(&status)
    ))
}

fn for_terminal(text: String) -> String {
    let wants_colour = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    if wants_colour { text } else { strip(&text) }
}

fn terminal_width() -> usize {
    if std::io::stdout().is_terminal()
        && let Ok((columns, _)) = crossterm::terminal::size()
    {
        return usize::from(columns);
    }
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|columns| *columns > 0)
        .unwrap_or(DEFAULT_WIDTH)
}

#[cfg(test)]
#[path = "command/stop_tests.rs"]
pub mod stop_tests;
