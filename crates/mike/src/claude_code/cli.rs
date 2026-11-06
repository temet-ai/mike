use clap::{Args, Subcommand};

use super::apply::{self, ShellRequest};
use super::error::ClaudeCodeError;
use super::locations::Locations;
use super::route::{self, Route};
use super::settings::FUNCTION_KEY;
use super::shell::Shell;
use super::{mcp, mods, settings, shell_name, status};
use crate::config::{CliOverrides, ProxyConfig};
use crate::finding::Finding;

#[derive(Debug, Clone, Args)]
pub struct ClaudeCodeArgs {
    #[command(subcommand)]
    pub command: ClaudeCodeCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ClaudeCodeCommand {
    Status,
    Mcp(McpArgs),
    Route(RouteArgs),
    Shell(ShellArgs),
    Mods(ModsArgs),
    Trust(TrustArgs),
}

#[derive(Debug, Clone, Args)]
pub struct McpArgs {
    #[arg(long)]
    pub remove: bool,
    #[arg(long)]
    pub no_check: bool,
}

#[derive(Debug, Clone, Args)]
pub struct RouteArgs {
    #[arg(value_enum)]
    pub route: Route,
}

#[derive(Debug, Clone, Args)]
pub struct ShellArgs {
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long = "shell", value_enum, value_delimiter = ',')]
    pub shells: Vec<Shell>,
    #[arg(long)]
    pub remove: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ModsArgs {
    #[arg(long)]
    pub with_wiretap: bool,
    #[arg(long)]
    pub remove: bool,
}

#[derive(Debug, Clone, Args)]
pub struct TrustArgs {
    #[arg(value_parser = ["on", "off"])]
    pub setting: String,
}

fn print_findings(findings: &[Finding]) -> i32 {
    let mut code = 0;
    for finding in findings {
        if finding.passed() {
            println!("{}", finding.text);
        } else {
            eprintln!("{}", finding.text);
            code = 1;
        }
    }
    code
}

pub async fn run(args: ClaudeCodeArgs, overrides: CliOverrides) -> i32 {
    match dispatch(args.command, overrides).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("mike claude-code: {e}");
            1
        }
    }
}

fn load_config(overrides: CliOverrides) -> Result<ProxyConfig, ClaudeCodeError> {
    crate::config::load(overrides).map_err(|e| ClaudeCodeError::Load(format!("{e:#}")))
}

async fn dispatch(
    command: ClaudeCodeCommand,
    overrides: CliOverrides,
) -> Result<i32, ClaudeCodeError> {
    let locations = Locations::from_environment()?;
    match command {
        ClaudeCodeCommand::Status => {
            print!("{}", status::gather(&locations).render(&locations));
            Ok(0)
        }
        ClaudeCodeCommand::Mcp(args) => mcp_command(&locations, &args, overrides).await,
        ClaudeCodeCommand::Route(args) => Ok(print_findings(&apply::choose_route(
            &locations, args.route,
        )?)),
        ClaudeCodeCommand::Shell(args) => shell_command(&locations, &args, overrides),
        ClaudeCodeCommand::Mods(args) => mods_command(&locations, &args),
        ClaudeCodeCommand::Trust(args) => Ok(print_findings(&apply::choose_trust(
            &locations,
            args.setting == "on",
        )?)),
    }
}

async fn mcp_command(
    locations: &Locations,
    args: &McpArgs,
    overrides: CliOverrides,
) -> Result<i32, ClaudeCodeError> {
    if args.remove {
        return Ok(print_findings(&apply::disconnect_mcp(locations)?));
    }
    let config = load_config(overrides)?;
    let url = mcp::inspect_url_for(&config);
    let findings = apply::connect_mcp(locations, &url, !args.no_check).await?;
    Ok(print_findings(&findings))
}

fn mods_command(locations: &Locations, args: &ModsArgs) -> Result<i32, ClaudeCodeError> {
    if args.remove {
        return Ok(print_findings(&apply::remove_mods(locations)?));
    }
    let root = mods::find_for_process().ok_or(ClaudeCodeError::NoModsSource)?;
    Ok(print_findings(&apply::add_mods(
        locations,
        &root,
        args.with_wiretap,
    )?))
}

fn shell_command(
    locations: &Locations,
    args: &ShellArgs,
    overrides: CliOverrides,
) -> Result<i32, ClaudeCodeError> {
    if args.remove {
        return Ok(print_findings(&apply::remove_functions(locations)?));
    }
    let config = load_config(overrides)?;
    let route = match route::stored(&locations.config_file)? {
        Some(route) => route,
        None => {
            apply::choose_route(locations, Route::Sidecar)?;
            println!(
                "No route was chosen yet, so mike saved `sidecar`. Change it with `mike claude-code route plain`."
            );
            Route::Sidecar
        }
    };
    let name = match &args.name {
        Some(name) => name.clone(),
        None => settings::read(&locations.config_file, FUNCTION_KEY)?
            .unwrap_or_else(|| shell_name::suggest(locations.path_var.as_deref())),
    };
    let shells = if args.shells.is_empty() {
        Shell::ALL
            .into_iter()
            .filter(|s| s.installed(locations.path_var.as_deref()))
            .collect()
    } else {
        args.shells.clone()
    };
    let request = ShellRequest {
        name,
        shells,
        route,
        proxy_addr: crate::doctor::connect_addr(&config.proxy_listen_addr),
    };
    let exe = std::env::current_exe().ok();
    let findings = apply::add_function(locations, &request, exe.as_deref())?;
    let code = print_findings(&findings);
    println!(
        "Open a new terminal, or run `source` on the file, to use `{}`.",
        request.name
    );
    Ok(code)
}
