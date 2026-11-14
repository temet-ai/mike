use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use crate::compress::cache_gate::CacheGate;
use crate::compress::shell_output::{
    self, ShellOutputLimits, ShellOutputPolicy, ShellOutputReduction,
};
use crate::compress::stage::{ARM_CONTROL, ARM_FORWARD};
use crate::feature_toggles::ResolvedToggles;
use crate::features_settings::ShellOutputCompressionOverlay;
use crate::pipeline::{RequestStage, StageBody, StageContext};
use crate::wire::manifest::{AppliedStage, StageOutcome, TargetEntry, sha256_hex};

pub struct ShellOutputCompressionStage;

pub const STAGE: &str = "shell_output_compression";
const REDUCER: &str = "shell_output";
const SHADOW_CANDIDATE: &str = "shell_output_shadow_candidate";
const FORWARDED: &str = "shell_output_forwarded";

pub fn session_limits(toggles: &ResolvedToggles) -> ShellOutputLimits {
    let settings = toggles.features.get::<ShellOutputCompressionOverlay>();
    ShellOutputLimits {
        min_result_bytes: settings.min_bytes,
        head_lines: settings.head_lines,
        tail_lines: settings.tail_lines,
        signal_context_lines: settings.signal_context_lines,
        signal_line_cap: settings.signal_line_cap,
        grace_turns: settings.grace_turns,
        ..ShellOutputLimits::default()
    }
}

pub fn session_arm(toggles: &ResolvedToggles, session_id: &str) -> &'static str {
    let settings = toggles.features.get::<ShellOutputCompressionOverlay>();
    if settings.forward() {
        return ARM_FORWARD;
    }
    let digest = sha256_hex(format!("{REDUCER}:{session_id}").as_bytes());
    let bucket = u64::from_str_radix(&digest[..8], 16).unwrap_or(0) % 100;
    if bucket < settings.forward_share as u64 {
        ARM_FORWARD
    } else {
        ARM_CONTROL
    }
}

struct Record {
    parameters: Value,
    targets: Vec<TargetEntry>,
    would_reduce: usize,
}

fn record(reduction: ShellOutputReduction, arm: &str, original_bytes: usize) -> Record {
    let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
    let mut targets = Vec::new();
    for target in reduction.stage.targets {
        if target.outcome == StageOutcome::Skipped {
            *skipped.entry(target.reason_code.clone()).or_default() += 1;
        } else {
            targets.push(target);
        }
    }
    let would_reduce = targets
        .iter()
        .filter(|t| t.outcome == StageOutcome::Applied)
        .count();
    let saved_bytes: usize = targets
        .iter()
        .filter(|t| t.outcome == StageOutcome::Applied)
        .map(|t| t.original_bytes.saturating_sub(t.sent_bytes))
        .sum();
    let parameters = serde_json::json!({
        "arm": arm,
        "targets": skipped.values().sum::<usize>() + targets.len(),
        "would_reduce": would_reduce,
        "saved_bytes": saved_bytes,
        "skipped": skipped,
        "original_bytes": original_bytes,
        "candidate_bytes": reduction.candidate.len(),
        "policy": reduction.stage.resolved_parameters,
    });
    Record {
        parameters,
        targets,
        would_reduce,
    }
}

fn applied_ids(targets: &[TargetEntry]) -> HashSet<String> {
    targets
        .iter()
        .filter(|t| t.outcome == StageOutcome::Applied)
        .filter_map(|t| t.tool_use_id.clone())
        .collect()
}

fn open_gate(raw: &[u8]) -> CacheGate {
    let parsed = serde_json::from_slice(raw).unwrap_or(Value::Null);
    CacheGate::open(REDUCER, raw, &parsed)
}

fn forward(body: &mut StageBody, reduction: ShellOutputReduction, original_bytes: usize) {
    let candidate = reduction.candidate.clone();
    let rec = record(reduction, ARM_FORWARD, original_bytes);
    body.rewrite_forwarded_only(
        candidate.clone(),
        AppliedStage {
            stage: STAGE,
            policy_id: STAGE,
            policy_version: shell_output::POLICY_VERSION,
            resolved_parameters: rec.parameters,
            new_bytes: &candidate,
            targets: rec.targets,
            reason_code: FORWARDED,
            detail: Some(format!(
                "forwarded: {} Bash result(s) shortened; the client's original body is kept as the request body",
                rec.would_reduce
            )),
        },
    );
}

fn decline(
    body: &mut StageBody,
    reduction: ShellOutputReduction,
    arm: &str,
    original_bytes: usize,
) {
    let candidate_found = reduction.stage.outcome == StageOutcome::Applied;
    let stage_reason = reduction.stage.reason_code.clone();
    let rec = record(reduction, arm, original_bytes);
    let detail = (rec.would_reduce > 0).then(|| {
        format!(
            "computed only: {} Bash result(s) would have been shortened; the request was forwarded unchanged, and each target below describes the candidate, not what was sent",
            rec.would_reduce
        )
    });
    body.decline_with(crate::wire::manifest::UnchangedStage {
        stage: STAGE,
        policy_id: STAGE,
        reason_code: if candidate_found {
            SHADOW_CANDIDATE
        } else {
            stage_reason.as_str()
        },
        resolved_parameters: rec.parameters,
        targets: rec.targets,
        detail,
    });
}

#[async_trait::async_trait]
impl RequestStage for ShellOutputCompressionStage {
    fn name(&self) -> &'static str {
        STAGE
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.session_toggles.shell_output_compression
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        let toggles = ctx.session_toggles;
        let arm = session_arm(toggles, ctx.session_id);
        let gate = (arm == ARM_FORWARD).then(|| open_gate(body.body()));
        let policy = ShellOutputPolicy::new(session_limits(toggles))
            .with_capture(body.original())
            .with_warm_cuts(gate.as_ref().and_then(CacheGate::warm_cuts));
        let reduction = shell_output::reduce_shell_output(body.body().as_ref(), &policy);
        if let Some(gate) = gate {
            gate.close(applied_ids(&reduction.stage.targets));
        }
        let original_bytes = body.body().len();
        if reduction.stage.outcome == StageOutcome::Applied && arm == ARM_FORWARD {
            forward(body, reduction, original_bytes);
            return;
        }
        decline(body, reduction, arm, original_bytes);
    }
}

#[cfg(test)]
#[path = "shell_stage_tests.rs"]
mod tests;
