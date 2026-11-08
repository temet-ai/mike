use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::Serialize;

use crate::config::{CliOverrides, ProxyConfig};

mod companion;
mod probe;
mod registration;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Args)]
pub struct McpArgs {
    #[command(subcommand)]
    pub command: McpCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum McpCommand {
    #[command(about = "Verify the companion and its real read-only stdio connection")]
    Probe(ProbeArgs),
    #[command(about = "Explicitly register the verified MCP companion in Claude Code")]
    Register(RegisterArgs),
    #[command(about = "Remove only the selected installation's unchanged MCP registration")]
    Unregister(UnregisterArgs),
}

#[derive(Debug, Clone, Args)]
pub struct ProbeArgs {
    #[arg(
        long,
        help = "Stable absolute path to the matching mike-mcp executable"
    )]
    pub binary: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct RegisterArgs {
    #[command(flatten)]
    pub probe: ProbeArgs,
    #[arg(long, help = "Claude JSON file (default: ~/.claude.json)")]
    pub claude_config: Option<PathBuf>,
    #[arg(
        long,
        help = "Explicitly replace a conflicting mike entry; retain a backup"
    )]
    pub replace: bool,
}

#[derive(Debug, Clone, Args)]
pub struct UnregisterArgs {
    #[arg(
        long,
        help = "Stable absolute path to the matching mike-mcp executable"
    )]
    pub binary: Option<PathBuf>,
    #[arg(long, help = "Claude JSON file (default: ~/.claude.json)")]
    pub claude_config: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistrationPlan {
    pub binary: PathBuf,
    pub inspect_url: String,
    pub instance_id: String,
    pub client_config: PathBuf,
}

impl RegistrationPlan {
    pub fn entry(&self) -> serde_json::Value {
        serde_json::json!({
            "command": self.binary,
            "args": ["--core"],
            "env": {
                "MIKE_INSPECT_URL": self.inspect_url,
                "MIKE_REGISTRATION_INSTANCE_ID": self.instance_id
            }
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeReport {
    pub binary: PathBuf,
    pub inspect_url: String,
    pub instance_id: String,
    pub version: &'static str,
    pub handshake: bool,
    pub backend_read: bool,
    pub code: &'static str,
    pub detail: &'static str,
}

#[derive(Debug, Serialize)]
pub struct RegistrationReport {
    pub client_config: PathBuf,
    pub changed: bool,
    pub backup: Option<PathBuf>,
    pub probe: ProbeReport,
}

#[derive(Debug, Serialize)]
pub struct UnregistrationReport {
    pub client_config: PathBuf,
    pub changed: bool,
    pub backup: Option<PathBuf>,
}

pub fn plan(
    config: &ProxyConfig,
    binary: Option<&Path>,
    client_config: Option<&Path>,
) -> Result<RegistrationPlan> {
    let client_config = match client_config {
        Some(path) => absolute(path)?,
        None => absolute(
            &PathBuf::from(
                std::env::var_os("HOME").context("HOME is unset; supply --claude-config")?,
            )
            .join(".claude.json"),
        )?,
    };
    Ok(RegistrationPlan {
        binary: companion::resolve(binary)?,
        inspect_url: inspect_url(config)?,
        instance_id: instance_id(config)?,
        client_config,
    })
}

fn inspect_url(config: &ProxyConfig) -> Result<String> {
    crate::config::endpoint::inspect_url(config)
}

fn instance_id(config: &ProxyConfig) -> Result<String> {
    let selected =
        (!config.config_path.as_os_str().is_empty()).then_some(config.config_path.as_path());
    Ok(crate::config::paths::InstallationPaths::resolve(selected)?.installation_id)
}

pub(crate) fn companion_binary() -> Result<PathBuf> {
    companion::resolve(None)
}

pub async fn check(config: &ProxyConfig) -> Result<ProbeReport> {
    let binary = companion_binary()?;
    probe::run(&binary, &inspect_url(config)?, &instance_id(config)?).await
}

pub async fn register(plan: RegistrationPlan, replace: bool) -> Result<RegistrationReport> {
    let prepared = registration::prepare(&plan, replace)?;
    let probe = probe::run(&plan.binary, &plan.inspect_url, &plan.instance_id).await?;
    admit(&probe)?;
    let (changed, backup) = prepared.commit()?;
    Ok(RegistrationReport {
        client_config: plan.client_config,
        changed,
        backup,
        probe,
    })
}

fn admit(probe: &ProbeReport) -> Result<()> {
    anyhow::ensure!(
        probe.handshake && (probe.backend_read || probe.code == "MCP_BACKEND_UNAVAILABLE"),
        "{}: {}",
        probe.code,
        probe.detail
    );
    Ok(())
}

pub fn unregister(plan: RegistrationPlan) -> Result<UnregistrationReport> {
    let (changed, backup) = registration::prepare_removal(&plan)?.commit()?;
    Ok(UnregistrationReport {
        client_config: plan.client_config,
        changed,
        backup,
    })
}

pub async fn run(args: McpArgs, overrides: CliOverrides) -> Result<()> {
    let config = crate::config::load(overrides)?;
    match args.command {
        McpCommand::Probe(args) => {
            let binary = companion::resolve(args.binary.as_deref())?;
            let report =
                probe::run(&binary, &inspect_url(&config)?, &instance_id(&config)?).await?;
            print_report(&report, args.json)?;
            anyhow::ensure!(report.backend_read, "MCP readiness check did not pass");
        }
        McpCommand::Register(args) => {
            let plan = plan(
                &config,
                args.probe.binary.as_deref(),
                args.claude_config.as_deref(),
            )?;
            let report = register(plan, args.replace).await?;
            if args.probe.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "MCP registration {}: {}",
                    if report.changed { "saved" } else { "unchanged" },
                    report.client_config.display()
                );
                if let Some(backup) = &report.backup {
                    println!("Previous config backup: {}", backup.display());
                }
                print_report(&report.probe, false)?;
            }
        }
        McpCommand::Unregister(args) => {
            let plan = plan(
                &config,
                args.binary.as_deref(),
                args.claude_config.as_deref(),
            )?;
            let report = unregister(plan)?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "MCP registration {}: {}",
                    if report.changed { "removed" } else { "absent" },
                    report.client_config.display()
                );
                if let Some(backup) = &report.backup {
                    println!("Previous config backup: {}", backup.display());
                }
            }
        }
    }
    Ok(())
}

fn print_report(report: &ProbeReport, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!("{}: {}", report.code, report.detail);
        println!("Companion: {}", report.binary.display());
        println!("Selected inspect endpoint: {}", report.inspect_url);
    }
    Ok(())
}

fn absolute(path: &Path) -> Result<PathBuf> {
    anyhow::ensure!(!path.as_os_str().is_empty(), "path must not be empty");
    anyhow::ensure!(
        path.to_str().is_some(),
        "MCP and Claude config paths must be valid UTF-8"
    );
    let path = std::path::absolute(path)?;
    let mut normalized = PathBuf::new();
    for part in path.components() {
        if part == std::path::Component::ParentDir {
            normalized.pop();
        } else if part != std::path::Component::CurDir {
            normalized.push(part.as_os_str());
        }
    }
    Ok(normalized)
}
