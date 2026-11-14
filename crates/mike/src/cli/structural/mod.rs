mod snapshot;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand};
use mike_code::wiring::{Kind, WiringFinding, WiringRules, check, counts, introduced};
use serde::Serialize;

pub const RULES_PATH: &str = ".mike/structural/wiring.yml";

#[derive(Debug, Clone, Args)]
pub struct StructuralArgs {
    #[command(subcommand)]
    pub command: StructuralCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum StructuralCommand {
    Wiring(WiringArgs),
}

#[derive(Debug, Clone, Args)]
pub struct WiringArgs {
    #[arg(
        long,
        value_name = "A..B",
        help = "Report only the findings B has and A does not"
    )]
    pub range: Option<String>,
    #[arg(long, help = "Print the report as JSON")]
    pub json: bool,
    #[arg(
        long,
        value_name = "FILE",
        help = "Wiring rules (default: .mike/structural/wiring.yml in the repository)"
    )]
    pub rules: Option<PathBuf>,
}

#[derive(Serialize)]
struct Report<'a> {
    scope: &'a str,
    new_only: bool,
    counts: BTreeMap<Kind, usize>,
    findings: &'a [WiringFinding],
}

pub fn run(args: StructuralArgs) -> Result<()> {
    let StructuralCommand::Wiring(wiring) = args.command;
    let root = snapshot::repo_root(&std::env::current_dir()?)?;
    let rules_path = wiring
        .rules
        .clone()
        .unwrap_or_else(|| root.join(RULES_PATH));
    let rules_text = std::fs::read_to_string(&rules_path)
        .with_context(|| format!("cannot read the wiring rules {}", rules_path.display()))?;
    match &wiring.range {
        Some(range) => {
            let findings = introduced_by(&root, range, &rules_text)?;
            print_report(range, true, &findings, wiring.json)
        }
        None => {
            let files = snapshot::working_tree(&root)?;
            let rules = WiringRules::from_yaml(&rules_text)?;
            print_report("whole tree", false, &check(&rules, &files), wiring.json)
        }
    }
}

fn introduced_by(root: &Path, range: &str, rules_text: &str) -> Result<Vec<WiringFinding>> {
    let (base, head) = split_range(range)?;
    let base = resolve_base(root, &base, &head)?;
    let (before, after) = std::thread::scope(|scope| -> Result<_> {
        let before = scope.spawn(|| findings_at(root, &base, rules_text));
        let after = findings_at(root, &head, rules_text);
        let before = before
            .join()
            .map_err(|_| anyhow!("the scan of {base} panicked"))??;
        Ok((before, after?))
    })?;
    Ok(introduced(&before, &after))
}

fn findings_at(root: &Path, revision: &str, rules_text: &str) -> Result<Vec<WiringFinding>> {
    let files = snapshot::at_revision(root, revision)
        .with_context(|| format!("cannot read the tree at {revision}"))?;
    let rules = WiringRules::from_yaml(rules_text)?;
    Ok(check(&rules, &files))
}

fn split_range(range: &str) -> Result<(String, String)> {
    let Some((base, head)) = range.split_once("..") else {
        bail!("range {range:?} must look like A..B");
    };
    let head = head.trim_start_matches('.');
    if base.is_empty() {
        bail!("range {range:?} has no base before the dots");
    }
    let head = if head.is_empty() { "HEAD" } else { head };
    Ok((base.to_owned(), head.to_owned()))
}

fn resolve_base(root: &Path, base: &str, head: &str) -> Result<String> {
    if snapshot::is_commit(root, base) {
        return Ok(base.to_owned());
    }
    for main in ["origin/main", "main"] {
        if snapshot::is_commit(root, main)
            && let Ok(fork) = snapshot::merge_base(root, main, head)
        {
            eprintln!(
                "structural wiring: {base} is not here; comparing with the merge-base of {main}"
            );
            return Ok(fork);
        }
    }
    bail!("base {base} is not a commit here; run git fetch")
}

fn print_report(scope: &str, new_only: bool, findings: &[WiringFinding], json: bool) -> Result<()> {
    let counted = counts(findings);
    if json {
        let report = Report {
            scope,
            new_only,
            counts: counted,
            findings,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    let label = if new_only { "new" } else { "total" };
    println!("wiring: {scope}: {} {label}", findings.len());
    if !new_only {
        for (kind, count) in &counted {
            println!("wiring:   {:<20} {count}", kind.as_str());
        }
    }
    for finding in findings {
        println!(
            "wiring: {} {}:{}: {}",
            finding.kind.as_str(),
            finding.path,
            finding.line,
            finding.message
        );
    }
    Ok(())
}
