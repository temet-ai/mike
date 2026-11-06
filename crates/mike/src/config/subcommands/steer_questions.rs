use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use serde_json::json;

use crate::config::CliOverrides;
use crate::steerer::guard::Question;
use crate::steerer::guard::holdback::{ArmAnchor, holdback_draw};
use crate::steerer::guard::policy::{GuardSettings, question_policies};
use crate::steerer::guard::promotion::effect::estimate_effect;
use crate::steerer::guard::promotion::service::{
    DemoteRequest, PromoteRequest, demote, gate_for, promote,
};
use crate::steerer::guard::promotion::status::{StatusRequest, questions_status};
use crate::store::persistence::PersistHandle;

#[derive(Debug, Clone, Subcommand)]
pub enum SteerQuestionsCommand {
    Status(StatusArgs),
    Effect(EffectArgs),
    VerifyArms(VerifyArgs),
    Promote(PromoteArgs),
    Demote(DemoteArgs),
}

#[derive(Debug, Clone, Args)]
pub struct StatusArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct EffectArgs {
    #[arg(long)]
    pub question: Option<String>,
    #[arg(long = "as-of")]
    pub as_of: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Args)]
pub struct VerifyArgs {
    #[arg(long)]
    pub question: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct PromoteArgs {
    #[arg(long)]
    pub question: String,
    #[arg(long = "to")]
    pub to_mode: String,
    #[arg(long = "override-reason")]
    pub override_reason: Option<String>,
    #[arg(long = "replaces-steerer")]
    pub replaces_steerer: Option<i64>,
    #[arg(long)]
    pub execute: bool,
}

#[derive(Debug, Clone, Args)]
pub struct DemoteArgs {
    #[arg(long)]
    pub question: String,
    #[arg(long = "to")]
    pub to_mode: String,
    #[arg(long)]
    pub reason: String,
    #[arg(long = "restore-steerer")]
    pub restore_steerer: Option<i64>,
    #[arg(long)]
    pub execute: bool,
}

fn question(raw: &str) -> Result<Question> {
    Question::parse(raw).with_context(|| format!("question {raw:?} is not one of relapse|unbacked_wait|authorised_handback|unobserved_done"))
}

async fn guard_id(persist: &PersistHandle) -> Result<i64> {
    Ok(persist
        .guard_steerer()
        .await?
        .context("the turn-guard steerer row is not seeded")?
        .id)
}

fn source_json() -> String {
    crate::change_log::ChangeSource::new(crate::change_log::ChangeVia::Mike)
        .with_detail("via", json!("cli"))
        .to_setting_json(&crate::change_log::current_build())
}

async fn status(args: StatusArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::eval::connect(cli, false).await?;
    let steerer_id = guard_id(&persist).await?;
    let body = questions_status(
        &persist,
        &StatusRequest {
            steerer_id,
            bundle_id: None,
            verifier_id: None,
            now: Utc::now(),
        },
    )
    .await?;
    let text = if args.json {
        serde_json::to_string_pretty(&body)?
    } else {
        serde_json::to_string(&body)?
    };
    println!("{text}");
    Ok(())
}

async fn effect(args: EffectArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::eval::connect(cli, false).await?;
    let steerer_id = guard_id(&persist).await?;
    let as_of = args.as_of.unwrap_or_else(Utc::now);
    let q = args.question.as_deref().map(question).transpose()?;
    let rows = persist
        .fetch_arm_rows(steerer_id, q.map(Question::code), as_of)
        .await?;
    let estimate = estimate_effect(
        &rows,
        &std::collections::HashMap::new(),
        q.map(Question::code),
        as_of.timestamp(),
    );
    println!("{}", serde_json::to_string_pretty(&estimate)?);
    Ok(())
}

async fn verify_arms(args: VerifyArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::eval::connect(cli, false).await?;
    let steerer_id = guard_id(&persist).await?;
    let q = args.question.as_deref().map(question).transpose()?;
    let policies = question_policies(&persist, steerer_id).await;
    let settings = GuardSettings::load(&persist).await;
    let rows = persist
        .fetch_arm_rows(steerer_id, q.map(Question::code), Utc::now())
        .await?;
    let mut mismatches = Vec::new();
    for r in &rows {
        let (Some(rq), Some(anchor), Some(stored)) = (
            r.question.as_deref().and_then(Question::parse),
            r.arm_anchor_turn_id,
            r.holdback_draw,
        ) else {
            continue;
        };
        let Some(policy) = policies.get(&rq) else {
            continue;
        };
        let draw = holdback_draw(
            policy.holdback_salt,
            &ArmAnchor {
                steerer_id,
                question: rq,
                anchor_turn_id: anchor,
            },
        );
        let pct = policy.effective_holdback(&settings) as f64 / 100.0;
        let arm = if draw < pct { "holdback" } else { "treated" };
        if (draw - stored).abs() > 1e-12 || r.arm.as_deref() != Some(arm) {
            mismatches.push(json!({"evaluation": r.uuid, "stored_draw": stored, "draw": draw, "stored_arm": r.arm, "arm": arm}));
        }
    }
    let report =
        json!({"rows": rows.len(), "mismatches": mismatches.len(), "rows_mismatched": mismatches});
    println!(
        "{}",
        if args.json {
            serde_json::to_string_pretty(&report)?
        } else {
            format!("{} arm rows, {} mismatches", rows.len(), mismatches.len())
        }
    );
    if !mismatches.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

async fn promote_cmd(args: PromoteArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::eval::connect(cli, args.execute).await?;
    let steerer_id = guard_id(&persist).await?;
    let q = question(&args.question)?;
    let gate = gate_for(&persist, steerer_id, q, &args.to_mode, None)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&gate.to_json())?);
    if !args.execute {
        println!(
            "dry run: pass --execute to promote {} to {}",
            q.code(),
            args.to_mode
        );
        return Ok(());
    }
    let source = source_json();
    let (id, _) = promote(
        &persist,
        &PromoteRequest {
            steerer_id,
            question: q,
            to_mode: &args.to_mode,
            override_reason: args.override_reason.as_deref(),
            replaces_steerer_id: args.replaces_steerer,
            decided_via: "cli",
            bundle_id: None,
            source_json: &source,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("promotion {id} recorded; the running server applies it within 60 s");
    Ok(())
}

async fn demote_cmd(args: DemoteArgs, cli: CliOverrides) -> Result<()> {
    let q = question(&args.question)?;
    if !args.execute {
        println!(
            "dry run: pass --execute to demote {} to {}",
            q.code(),
            args.to_mode
        );
        return Ok(());
    }
    let persist = super::eval::connect(cli, true).await?;
    let steerer_id = guard_id(&persist).await?;
    let source = source_json();
    let id = demote(
        &persist,
        &DemoteRequest {
            steerer_id,
            question: q,
            to_mode: &args.to_mode,
            reason: &args.reason,
            restore_steerer_id: args.restore_steerer,
            decided_via: "cli",
            evidence: json!({"operator_reason": args.reason}),
            source_json: &source,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("demotion {id} recorded");
    Ok(())
}

pub async fn run(command: SteerQuestionsCommand, cli: CliOverrides) -> Result<()> {
    match command {
        SteerQuestionsCommand::Status(args) => status(args, cli).await,
        SteerQuestionsCommand::Effect(args) => effect(args, cli).await,
        SteerQuestionsCommand::VerifyArms(args) => verify_arms(args, cli).await,
        SteerQuestionsCommand::Promote(args) => promote_cmd(args, cli).await,
        SteerQuestionsCommand::Demote(args) => demote_cmd(args, cli).await,
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::SteerQuestionsCommand;

    #[derive(Parser)]
    struct Cli {
        #[command(subcommand)]
        command: SteerQuestionsCommand,
    }

    #[test]
    fn steer_questions_promote_defaults_to_dry_run() -> anyhow::Result<()> {
        let Cli { command } =
            Cli::try_parse_from(["x", "promote", "--question", "relapse", "--to", "append"])?;
        assert!(matches!(command, SteerQuestionsCommand::Promote(ref a) if !a.execute));
        assert!(Cli::try_parse_from(["x", "verify-arms", "--json"]).is_ok());
        Ok(())
    }
}
