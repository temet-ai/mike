use anyhow::Context as _;
use clap::{Args, Subcommand};
use serde_json::{Value, json};

use crate::config::CliOverrides;

#[derive(Debug, Clone, Args)]
pub struct FindingsArgs {
    #[command(subcommand)]
    pub command: FindingsCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum FindingsCommand {
    #[command(about = "List this session's open findings")]
    List,
    #[command(about = "Keep the flagged code, with the reason it is right")]
    Keep {
        id: i64,
        #[arg(long)]
        reason: String,
    },
    #[command(about = "Say the flagged change is not this session's")]
    NotMine { id: i64 },
}

fn session() -> anyhow::Result<String> {
    std::env::var("CLAUDE_CODE_SESSION_ID")
        .context("run this inside a Claude Code session (CLAUDE_CODE_SESSION_ID is not set)")
}

pub async fn run(args: FindingsArgs, overrides: CliOverrides) -> anyhow::Result<i32> {
    let config = crate::config::load(overrides)?;
    let base = crate::config::endpoint::inspect_url(&config)?;
    let client = reqwest::Client::new();
    let session = session()?;
    let (id, body) = match args.command {
        FindingsCommand::List => return list(&client, &base, &session).await,
        FindingsCommand::Keep { id, reason } => (
            id,
            json!({"session": session, "claim": "keep", "reason": reason}),
        ),
        FindingsCommand::NotMine { id } => (id, json!({"session": session, "claim": "not_mine"})),
    };
    let ruling: Value = client
        .post(format!("{base}/api/code-findings/{id}/answer"))
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let text = |key: &str| ruling.get(key).and_then(Value::as_str).unwrap_or_default();
    println!("{}", text("message"));
    Ok(i32::from(matches!(text("answer"), "refused" | "gone")))
}

async fn list(client: &reqwest::Client, base: &str, session: &str) -> anyhow::Result<i32> {
    let body: Value = client
        .get(format!(
            "{base}/api/code-findings?session={session}&hours=168&limit=1000"
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let lines = open_lines(&body);
    if lines.is_empty() {
        println!("No open findings.");
    }
    for line in lines {
        println!("{line}");
    }
    Ok(0)
}

fn open_lines(body: &Value) -> Vec<String> {
    let text = |f: &Value, key: &str| {
        f.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let mut seen = std::collections::BTreeSet::new();
    let findings = body.get("findings").and_then(Value::as_array);
    findings
        .into_iter()
        .flatten()
        .rev()
        .filter(|f| matches!(text(f, "outcome").as_str(), "open" | "ignored"))
        .filter(|f| text(f, "severity") != "hint")
        .filter(|f| seen.insert((text(f, "checkout"), text(f, "path"), text(f, "rule_id"))))
        .map(|f| {
            let state = match text(f, "answer").as_str() {
                "waiting" => " (with the operator)",
                "refused" => " (reason refused)",
                _ => "",
            };
            format!(
                "[{}] {}:{} {}: {}{state}",
                f.get("id").and_then(Value::as_i64).unwrap_or_default(),
                text(f, "path"),
                f.get("line").and_then(Value::as_i64).unwrap_or_default(),
                text(f, "rule_id"),
                text(f, "message")
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_one_line_per_file_and_rule_and_leaves_out_closed_findings() {
        let f = |id: i64, outcome: &str, answer: Value| {
            json!({"id": id, "checkout": "/c", "path": "a.sh", "line": 3, "rule_id": "r",
                   "severity": "warning", "message": "m", "outcome": outcome, "answer": answer})
        };
        let body = json!({"findings": [
            f(3, "fixed", Value::Null),
            f(2, "open", Value::Null),
            f(1, "open", json!("waiting")),
        ]});
        assert_eq!(
            open_lines(&body),
            vec!["[1] a.sh:3 r: m (with the operator)"]
        );
    }
}
