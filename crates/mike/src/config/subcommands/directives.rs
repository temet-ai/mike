use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use clap::{Args, Subcommand};

use crate::config::CliOverrides;
use crate::cross_session::CrossSessionSnapshot;
use crate::cross_session::backtest::{BacktestParams, TagEvent, qualifying_pairs, summarise};
use crate::cross_session::proposer::run_proposer_pass;

#[derive(Debug, Clone, Subcommand)]
pub enum DirectivesCommand {
    Backtest(BacktestArgs),
    Propose(ProposeArgs),
}

#[derive(Debug, Clone, Args)]
pub struct BacktestArgs {
    #[arg(long)]
    pub corrections: PathBuf,
    #[arg(long)]
    pub turns: PathBuf,
    #[arg(long = "min-turns", default_value_t = 4)]
    pub min_turns: usize,
    #[arg(long = "min-sessions", default_value_t = 3)]
    pub min_sessions: usize,
    #[arg(long = "min-major", default_value_t = 2)]
    pub min_major: usize,
    #[arg(long = "window-days", default_value_t = 30)]
    pub window_days: i64,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ProposeArgs {
    #[arg(long)]
    pub execute: bool,
}

type TurnMeta = (String, String, String, DateTime<Utc>);

fn parse_time(raw: &str) -> Option<DateTime<Utc>> {
    let t = raw.trim().replace(' ', "T");
    DateTime::parse_from_rfc3339(&t)
        .map(|d| d.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(&t, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|n| n.and_utc())
        })
}

fn load_turns(path: &PathBuf) -> Result<HashMap<i64, TurnMeta>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            let [id, session, project, resolution, closed] = parts.as_slice() else {
                return None;
            };
            Some((
                id.parse().ok()?,
                (
                    (*session).to_owned(),
                    (*project).to_owned(),
                    (*resolution).to_owned(),
                    parse_time(closed)?,
                ),
            ))
        })
        .collect())
}

type EventsByKey = BTreeMap<(String, String), Vec<TagEvent>>;

fn load_events(
    corrections: &PathBuf,
    turns: &HashMap<i64, TurnMeta>,
) -> Result<(EventsByKey, usize)> {
    let text = std::fs::read_to_string(corrections)
        .with_context(|| format!("reading {}", corrections.display()))?;
    let mut events: EventsByKey = BTreeMap::new();
    let mut missing = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: serde_json::Value =
            serde_json::from_str(line).context("parsing a correction row")?;
        let Some((session, project, resolution, closed_at)) =
            row["id"].as_i64().and_then(|id| turns.get(&id))
        else {
            missing += 1;
            continue;
        };
        let scope = if resolution == "unresolved" {
            "no_project".to_owned()
        } else {
            format!("project:{project}")
        };
        let major = matches!(row["sev"].as_str(), Some("maj" | "sev"));
        for tag in row["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t.as_str())
        {
            if tag.starts_with("new:") {
                continue;
            }
            events
                .entry((scope.clone(), tag.to_owned()))
                .or_default()
                .push(TagEvent {
                    closed_at: *closed_at,
                    session: session.clone(),
                    major,
                });
        }
    }
    Ok((events, missing))
}

async fn backtest(args: BacktestArgs) -> Result<()> {
    let turns = load_turns(&args.turns)?;
    let (events, missing) = load_events(&args.corrections, &turns)?;
    let params = BacktestParams {
        min_turns: args.min_turns,
        min_sessions: args.min_sessions,
        min_major: args.min_major,
        window_days: args.window_days,
    };
    let q = qualifying_pairs(&events, &params);
    let summary = summarise(&q);
    let pairs: Vec<serde_json::Value> = q
        .iter()
        .map(|((scope, tag), at)| serde_json::json!({"scope": scope, "tag": tag, "qualified_at": at}))
        .collect();
    let body = serde_json::json!({"corrections_without_a_resolved_turn": missing, "summary": summary, "pairs": pairs});
    println!(
        "{}",
        if args.json {
            serde_json::to_string_pretty(&body)?
        } else {
            serde_json::to_string(&body)?
        }
    );
    Ok(())
}

async fn propose(args: ProposeArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::eval::connect(cli, args.execute).await?;
    let snapshot = CrossSessionSnapshot::load_now(&persist).await?;
    let Some(rollup) = snapshot.rollup else {
        anyhow::bail!("no finished eval.rollups generation; run mike eval rollup first");
    };
    if !args.execute {
        println!(
            "dry run: would run the proposer on rollup {} with {:?}",
            rollup.id, snapshot.settings
        );
        return Ok(());
    }
    let report = run_proposer_pass(&persist, &snapshot.settings, rollup.id).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run(command: DirectivesCommand, cli: CliOverrides) -> Result<()> {
    match command {
        DirectivesCommand::Backtest(args) => backtest(args).await,
        DirectivesCommand::Propose(args) => propose(args, cli).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_time_accepts_psql_timestamps() {
        assert!(parse_time("2026-09-01 10:00:00+00:00").is_some());
        assert!(parse_time("not a time").is_none());
        assert!(parse_time("2026-09-01T10:00:00Z").is_some());
        assert!(parse_time("2026-09-01 10:00:00.123").is_some());
    }
}
