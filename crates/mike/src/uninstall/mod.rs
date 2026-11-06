mod purge;
#[cfg(test)]
mod tests;

use anyhow::Context;
use clap::Args;

use crate::claude_code::{
    ClaudeCodeError, FUNCTION_KEY, Locations, ROUTE_KEY, SETUP_KEY, disconnect_mcp,
    remove_functions, remove_mods, remove_settings,
};
use crate::config::CliOverrides;
use crate::db_provision::SystemRunner;
use crate::finding::Finding;
use crate::service::{DryRunArgs, ServiceArgs, ServiceCommand};

#[derive(Debug, Clone, Args)]
pub struct UninstallArgs {
    #[arg(long)]
    pub purge: bool,
}

pub async fn run(args: UninstallArgs, overrides: CliOverrides) -> i32 {
    match uninstall(&args, overrides).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("mike uninstall: {e:#}");
            1
        }
    }
}

pub(crate) fn remove_connection(locations: &Locations) -> Result<Vec<Finding>, ClaudeCodeError> {
    let mut findings = disconnect_mcp(locations)?;
    findings.extend(remove_functions(locations)?);
    findings.extend(remove_mods(locations)?);
    remove_settings(
        &locations.config_file,
        &[ROUTE_KEY, FUNCTION_KEY, SETUP_KEY],
    )?;
    findings.push(Finding::ok(format!(
        "Removed the route, the function name and the setup state from {}.",
        locations.config_file.display()
    )));
    Ok(findings)
}

fn print(findings: &[Finding]) -> i32 {
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

async fn uninstall(args: &UninstallArgs, overrides: CliOverrides) -> anyhow::Result<i32> {
    let locations = Locations::from_environment().context("cannot find the home folder")?;
    let mut code = print(&remove_connection(&locations)?);
    let service_args = ServiceArgs {
        command: ServiceCommand::Uninstall(DryRunArgs { dry_run: false }),
    };
    code |= crate::service::run(service_args, overrides).await;
    if args.purge {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        let mut output = std::io::stdout();
        code |= purge::purge(
            &SystemRunner,
            &locations.config_file,
            &mut input,
            &mut output,
        )
        .await?;
    } else {
        println!(
            "Kept the database, its data volume and the config file at {}. `mike uninstall --purge` asks about removing them.",
            locations.config_file.display()
        );
    }
    Ok(code)
}
