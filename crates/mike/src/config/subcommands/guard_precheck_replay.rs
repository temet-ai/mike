use std::io::Write as _;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Args;
use serde_json::json;

use super::decisions::RecordingClient;
use crate::config::CliOverrides;
use crate::dossier::steering_facts::SteeringFacts;
use crate::llm::decisions::record::{DecisionContext, DecisionMode, DecisionSubject};
use crate::steerer::guard::Question;
use crate::steerer::precheck::score::{auroc, cut};
use crate::steerer::precheck::state::{PrecheckInput, state};
use crate::steerer::precheck::{self, POINT, Precheck};
use crate::store::persistence::PersistHandle;

#[derive(Debug, Clone, Args)]
pub struct PrecheckReplayArgs {
    #[arg(long)]
    pub since: Option<DateTime<Utc>>,
    #[arg(long)]
    pub dump: Option<PathBuf>,
    #[arg(long)]
    pub out: Option<PathBuf>,
    #[arg(long, default_value_t = crate::settings::meta_decisions::DEFAULT_TURN_GUARD_HOLD_BELOW)]
    pub hold_below: f64,
}

#[derive(Debug, sqlx::FromRow)]
struct Fire {
    uuid: uuid::Uuid,
    question: String,
    label: String,
    confidence: Option<f64>,
    reason: Option<String>,
    session_id: i64,
    operator_turn_id: i64,
    dispatched_at: DateTime<Utc>,
    ask: Option<String>,
    final_text: Option<String>,
}

const FIRES_SQL: &str =
    "SELECT e.uuid, e.question, e.outcome_label AS label, e.confidence, e.reason,
        e.session_id, e.operator_turn_id, e.dispatched_at,
        ot.user_prompt AS ask, ot.final_assistant_text AS final_text
   FROM steering.evaluations e
   JOIN steering.steerers st ON st.id = e.steerer_id
   JOIN memory.operator_turns ot ON ot.id = e.operator_turn_id
  WHERE st.name = 'turn-guard' AND e.status = 'resolved_correction'
    AND e.outcome_label IN ('right', 'wrong') AND ot.final_assistant_text IS NOT NULL
    AND e.dispatched_at >= $1
  ORDER BY e.id";

async fn fires(persist: &PersistHandle, since: DateTime<Utc>) -> Result<Vec<Fire>> {
    Ok(sqlx::query_as::<_, Fire>(FIRES_SQL)
        .bind(since)
        .fetch_all(persist.pool())
        .await?)
}

struct Replayed {
    fire: Fire,
    facts: Option<SteeringFacts>,
}

async fn with_facts(persist: &PersistHandle, fires: Vec<Fire>) -> Vec<Replayed> {
    let mut out = Vec::with_capacity(fires.len());
    for fire in fires {
        let facts = SteeringFacts::as_of(
            persist,
            fire.session_id,
            fire.operator_turn_id,
            fire.dispatched_at,
        )
        .await
        .ok();
        out.push(Replayed { fire, facts });
    }
    out
}

fn input<'a>(r: &'a Replayed, question: Question) -> PrecheckInput<'a> {
    PrecheckInput {
        question,
        steerer_reason: r.fire.reason.as_deref().unwrap_or(""),
        operator_ask: r.fire.ask.as_deref().unwrap_or(""),
        final_message: r.fire.final_text.as_deref().unwrap_or(""),
        facts: r.facts.as_ref(),
    }
}

fn write_lines(path: &PathBuf, lines: &[serde_json::Value]) -> Result<()> {
    let mut file =
        std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    for line in lines {
        writeln!(file, "{line}")?;
    }
    Ok(())
}

fn dump(replayed: &[Replayed], path: &PathBuf) -> Result<()> {
    let lines: Vec<_> = replayed
        .iter()
        .filter_map(|r| {
            let q = Question::parse(&r.fire.question)?;
            Some(json!({
                "evaluation_uuid": r.fire.uuid,
                "question": r.fire.question,
                "label": r.fire.label,
                "steerer_confidence": r.fire.confidence,
                "state": state(&input(r, q)),
                "questions": precheck::questions::questions_for(q),
            }))
        })
        .collect();
    write_lines(path, &lines)?;
    eprintln!("wrote {} states to {}", lines.len(), path.display());
    Ok(())
}

fn context(r: &Replayed) -> DecisionContext {
    let mut c = DecisionContext::new(
        POINT,
        DecisionMode::Replay,
        precheck::questions::QUESTION_SET,
    );
    c.facts = r.facts.as_ref().and_then(|f| serde_json::to_value(f).ok());
    c.subject = DecisionSubject {
        session_id: Some(r.fire.session_id),
        operator_turn_id: Some(r.fire.operator_turn_id),
        evaluation_uuid: Some(r.fire.uuid),
        ..DecisionSubject::default()
    };
    c.source_ref = Some(format!(
        "precheck-replay:{}:{}",
        precheck::questions::QUESTION_SET,
        r.fire.uuid
    ));
    c
}

fn report(results: &[(String, bool, Precheck, Option<f64>)]) {
    let questions: std::collections::BTreeSet<&str> =
        results.iter().map(|(q, ..)| q.as_str()).collect();
    for q in questions {
        let rows: Vec<_> = results.iter().filter(|(x, ..)| x == q).collect();
        let composite: Vec<(f64, bool)> = rows
            .iter()
            .map(|(_, y, p, _)| (p.verdict.score, *y))
            .collect();
        let steerer: Vec<(f64, bool)> = rows
            .iter()
            .map(|(_, y, _, c)| (c.unwrap_or(0.0), *y))
            .collect();
        println!(
            "{q}: fires {} right {}  AUROC steerer {:?} composite {:?}",
            rows.len(),
            rows.iter().filter(|r| r.1).count(),
            auroc(&steerer),
            auroc(&composite)
        );
        let names: std::collections::BTreeSet<&String> = rows
            .iter()
            .flat_map(|(_, _, p, _)| p.answers.keys())
            .collect();
        for name in names {
            let s: Vec<(f64, bool)> = rows
                .iter()
                .filter_map(|(_, y, p, _)| p.answers.get(name).map(|v| (*v, *y)))
                .collect();
            println!("  AUROC {name:24} {:?}", auroc(&s));
        }
        for t in [0.2, 0.3, 0.4, 0.5, 0.6] {
            println!("  {}", json!(cut(&composite, t)));
        }
    }
}

pub async fn run(args: PrecheckReplayArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::eval::connect(cli.clone(), false).await?;
    let since = args.since.unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let replayed = with_facts(&persist, fires(&persist, since).await?).await;
    if let Some(path) = &args.dump {
        return dump(&replayed, path);
    }
    let recording = RecordingClient::connect(cli, &persist).await?;
    let mut results = Vec::new();
    for r in &replayed {
        let Some(q) = Question::parse(&r.fire.question) else {
            continue;
        };
        if let Some(p) =
            precheck::run(&recording.client, &input(r, q), context(r), args.hold_below).await
        {
            results.push((
                r.fire.question.clone(),
                r.fire.label == "right",
                p,
                r.fire.confidence,
            ));
        }
    }
    recording.flush().await;
    report(&results);
    if let Some(path) = &args.out {
        let lines: Vec<_> = results
            .iter()
            .map(|(q, y, p, c)| json!({"question": q, "right": y, "steerer_confidence": c, "precheck": p}))
            .collect();
        write_lines(path, &lines)?;
    }
    Ok(())
}
