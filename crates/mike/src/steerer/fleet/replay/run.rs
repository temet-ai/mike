use crate::store::persistence::steerer_repo::SteererPersist as _;
use std::sync::Arc;

use serde::Serialize;

use super::ReplayItem;
use super::as_of::{AsOfInputs, assemble};
use super::sink::FileSink;
use crate::llm::OpenRouterClient;
use crate::settings::apply_db_rows_to_overlay;
use crate::steerer::fleet::input_budget::VerifierInputBudget;
use crate::steerer::fleet::judge::{apply_evidence_gate, confirm_quorum};
use crate::steerer::fleet::steerer_eval::pattern_gate_admits;
use crate::steerer::note_policy::{self, NoteLimits, SessionNotes, VerdictStop};
use crate::steerer::refocus::resolve_verdict_system_prompt;
use crate::steerer::registry::{Steerer, SteererRegistry};
use crate::steerer::verdict::{SteererVerdict, steerer_verdict_schema};
use crate::store::persistence::{PersistError, PersistHandle};
use crate::wire::machine_text;

const CHARS_PER_TOKEN: f64 = 3.25;

pub(crate) struct ReplaySteerer {
    pub(crate) steerer: Steerer,
    pub(crate) system: String,
    pub(crate) note_limits: NoteLimits,
}

pub(crate) enum ReplayMode<'a> {
    DryRun,
    Execute {
        client: &'a OpenRouterClient,
        sink: &'a Arc<FileSink>,
        max_usd: f64,
    },
}

impl ReplayMode<'_> {
    pub(super) fn exhausted(&self) -> bool {
        match self {
            Self::DryRun => false,
            Self::Execute { sink, max_usd, .. } => {
                sink.cost_unknown() || sink.total_usd() >= *max_usd
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ItemOutcome {
    DryRun,
    GateClosed,
    MachineAsk {
        asked_by: &'static str,
    },
    NoFire,
    BelowThreshold,
    NeedBelowLevel,
    QuorumMissed {
        votes_fire: usize,
        votes_total: usize,
    },
    Fired {
        votes_fire: usize,
        votes_total: usize,
    },
    CallFailed {
        error: String,
    },
    LoadFailed {
        error: String,
    },
    SkippedSpendCap,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct SectionSizes {
    pub(crate) user_message: usize,
    pub(crate) eval_text: usize,
    pub(crate) operator_ask: usize,
    pub(crate) accumulated_ask: usize,
    pub(crate) trajectory_digest: usize,
    pub(crate) session_action_list: usize,
    pub(crate) prior_fires: usize,
    pub(crate) session_block: usize,
}

impl SectionSizes {
    pub(crate) fn projected_prompt_tokens(&self) -> u64 {
        (self.user_message as f64 / CHARS_PER_TOKEN).ceil() as u64
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ItemReport {
    pub(crate) turn_id: i64,
    pub(crate) trigger_request_id: i64,
    pub(crate) gate_admitted: bool,
    pub(crate) sizes: SectionSizes,
    pub(crate) outcome: ItemOutcome,
    pub(crate) verdict: Option<SteererVerdict>,
    pub(crate) limit_at_the_time: Option<&'static str>,
    pub(crate) known_differences: &'static [&'static str],
    #[serde(skip)]
    pub(crate) user_message: String,
}

const KNOWN_DIFFERENCES: &[&str] = &[
    "session context is the current goal, not as-of",
    "thread criteria are discovered from the worklog as it is now",
    "diagnostics findings are not rebuilt",
    "the judge is always called; limit_at_the_time says whether the notes sent at the time would have stopped it first",
];

pub(crate) struct Prepared {
    pub(crate) message: String,
    pub(crate) admitted: bool,
    pub(crate) machine_ask: Option<&'static str>,
    pub(crate) notes: Option<SessionNotes>,
}

pub(crate) async fn load_replay_steerer(ph: &PersistHandle) -> Result<ReplaySteerer, PersistError> {
    let registry = SteererRegistry::from_rows(ph.steerer().list_steerers().await?);
    let steerer = registry
        .steerers
        .iter()
        .filter(|s| s.kind == "verifier")
        .max_by_key(|s| s.enabled)
        .cloned()
        .ok_or_else(|| {
            PersistError::DataInvariant("verifier replay: no verifier steerer row exists".into())
        })?;
    let system =
        resolve_verdict_system_prompt(Some(ph), steerer.agent_id, steerer.prompt.as_deref()).await;
    let settings = apply_db_rows_to_overlay(ph.fetch_settings().await?);
    Ok(ReplaySteerer {
        steerer,
        system,
        note_limits: NoteLimits::from_settings(&settings),
    })
}

fn sizes_of(inputs: &AsOfInputs) -> SectionSizes {
    let chars = |s: &str| s.chars().count();
    SectionSizes {
        user_message: chars(&inputs.user_message),
        eval_text: chars(&inputs.eval_inputs.eval_text),
        operator_ask: chars(&inputs.eval_inputs.operator_ask),
        accumulated_ask: chars(&inputs.context.accumulated_ask),
        trajectory_digest: chars(&inputs.context.trajectory_digest),
        session_action_list: chars(&inputs.context.session_action_list),
        prior_fires: chars(&inputs.context.prior_fires_digest),
        session_block: chars(&inputs.session_block),
    }
}

async fn judge(
    client: &OpenRouterClient,
    replay: &ReplaySteerer,
    prepared: &Prepared,
) -> (ItemOutcome, Option<SteererVerdict>) {
    let message = prepared.message.as_str();
    let mut verdict = match client
        .verdict::<SteererVerdict>(&replay.system, message, steerer_verdict_schema())
        .await
    {
        Ok(v) => v,
        Err(e) => {
            return (
                ItemOutcome::CallFailed {
                    error: e.to_string(),
                },
                None,
            );
        }
    };
    apply_evidence_gate(&mut verdict);
    let threshold = f64::from(replay.steerer.params.confidence_threshold);
    let stopped =
        match note_policy::stop_before_quorum(&verdict, threshold, prepared.notes.as_ref()) {
            Some(VerdictStop::Declined) => Some(ItemOutcome::NoFire),
            Some(VerdictStop::ConfidenceBelowThreshold) => Some(ItemOutcome::BelowThreshold),
            Some(VerdictStop::Note(_)) => Some(ItemOutcome::NeedBelowLevel),
            None => None,
        };
    if let Some(outcome) = stopped {
        return (outcome, Some(verdict));
    }
    let (votes_fire, votes_total) = confirm_quorum(client, &replay.system, message).await;
    let outcome = if votes_fire >= 2 {
        ItemOutcome::Fired {
            votes_fire,
            votes_total,
        }
    } else {
        ItemOutcome::QuorumMissed {
            votes_fire,
            votes_total,
        }
    };
    (outcome, Some(verdict))
}

pub(crate) async fn evaluate_prepared(
    mode: &ReplayMode<'_>,
    replay: &ReplaySteerer,
    prepared: &Prepared,
) -> (ItemOutcome, Option<SteererVerdict>) {
    match mode {
        ReplayMode::DryRun => (ItemOutcome::DryRun, None),
        ReplayMode::Execute { client, .. } => {
            if !prepared.admitted {
                return (ItemOutcome::GateClosed, None);
            }
            if let Some(asked_by) = prepared.machine_ask {
                return (ItemOutcome::MachineAsk { asked_by }, None);
            }
            judge(client, replay, prepared).await
        }
    }
}

fn failed_report(item: &ReplayItem, outcome: ItemOutcome) -> ItemReport {
    ItemReport {
        turn_id: item.turn_id,
        trigger_request_id: item.trigger_request_id,
        gate_admitted: false,
        sizes: SectionSizes::default(),
        outcome,
        verdict: None,
        limit_at_the_time: None,
        known_differences: &[],
        user_message: String::new(),
    }
}

async fn replay_one(
    ph: &PersistHandle,
    replay: &ReplaySteerer,
    item: &ReplayItem,
    budget: VerifierInputBudget,
    mode: &ReplayMode<'_>,
) -> ItemReport {
    let inputs = match assemble(ph, item, replay, budget).await {
        Ok(inputs) => inputs,
        Err(e) => {
            return failed_report(
                item,
                ItemOutcome::LoadFailed {
                    error: e.to_string(),
                },
            );
        }
    };
    let prepared = Prepared {
        admitted: pattern_gate_admits(&replay.steerer, &inputs.eval_inputs),
        machine_ask: machine_text::classify(&inputs.eval_inputs.operator_ask)
            .map(|kind| kind.in_words()),
        message: inputs.user_message.clone(),
        notes: Some(inputs.notes.clone()),
    };
    let limit_at_the_time = note_policy::before_judging(prepared.notes.as_ref(), replay.steerer.id)
        .map(note_policy::NoteRefusal::code);
    let (outcome, verdict) = evaluate_prepared(mode, replay, &prepared).await;
    ItemReport {
        turn_id: item.turn_id,
        trigger_request_id: item.trigger_request_id,
        gate_admitted: prepared.admitted,
        sizes: sizes_of(&inputs),
        outcome,
        verdict,
        limit_at_the_time,
        known_differences: KNOWN_DIFFERENCES,
        user_message: inputs.user_message,
    }
}

pub(crate) async fn replay_items(
    ph: &PersistHandle,
    replay: &ReplaySteerer,
    items: &[ReplayItem],
    budget: VerifierInputBudget,
    mode: &ReplayMode<'_>,
) -> Vec<ItemReport> {
    let mut reports = Vec::with_capacity(items.len());
    for item in items {
        if mode.exhausted() {
            reports.push(failed_report(item, ItemOutcome::SkippedSpendCap));
            continue;
        }
        reports.push(replay_one(ph, replay, item, budget, mode).await);
    }
    reports
}
