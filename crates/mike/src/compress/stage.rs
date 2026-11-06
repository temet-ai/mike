use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use crate::compress::cache_gate::CacheGate;
use crate::compress::file_reads;
use crate::compress::manifest::{
    AppliedStage, StageEntry, StageOutcome, TargetEntry, UnchangedStage,
};
use crate::feature_toggles::ResolvedToggles;
use crate::pipeline::{RequestStage, StageBody, StageContext};

pub(crate) struct FileReadCompressionStage;

const STAGE: &str = "file_read_compression";
const SHADOW_CANDIDATE: &str = "file_read_shadow_candidate";
const FORWARDED: &str = "file_read_forwarded";

struct ShadowRecord {
    parameters: Value,
    targets: Vec<TargetEntry>,
    detail: Option<String>,
}

fn shadow_record(stage: StageEntry, original_bytes: usize, candidate_bytes: usize) -> ShadowRecord {
    let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
    let mut recorded = Vec::new();
    for target in stage.targets {
        if target.outcome == StageOutcome::Skipped {
            *skipped.entry(target.reason_code.clone()).or_default() += 1;
        } else {
            recorded.push(target);
        }
    }
    let would_reduce = recorded
        .iter()
        .filter(|t| t.outcome == StageOutcome::Applied)
        .count();
    let total = skipped.values().sum::<usize>() + recorded.len();
    let parameters = serde_json::json!({
        "targets": total,
        "would_reduce": would_reduce,
        "skipped": skipped,
        "original_bytes": original_bytes,
        "candidate_bytes": candidate_bytes,
        "policy": stage.resolved_parameters,
    });
    let detail = (!recorded.is_empty()).then(|| {
        format!(
            "computed only: {would_reduce} read result(s) would have been shortened; the request was forwarded unchanged, and each target below describes the candidate, not what was sent"
        )
    });
    ShadowRecord {
        parameters,
        targets: recorded,
        detail,
    }
}

pub(crate) fn session_policy(toggles: &ResolvedToggles) -> file_reads::FileReadPolicy<'_> {
    file_reads::FileReadPolicy::new(
        toggles.file_read_compression_min_bytes,
        (
            toggles.file_read_compression_head_lines,
            toggles.file_read_compression_tail_lines,
        ),
        toggles.file_read_compression_grace_turns,
        &toggles.file_read_compression_enabled_producers,
        &toggles.file_read_compression_protected_paths,
    )
}

pub(crate) const ARM_FORWARD: &str = "forward";
pub(crate) const ARM_CONTROL: &str = "control";

pub(crate) fn session_arm(toggles: &ResolvedToggles, session_id: &str) -> &'static str {
    if toggles.file_read_compression_forward {
        return ARM_FORWARD;
    }
    let digest = crate::compress::manifest::sha256_hex(session_id.as_bytes());
    let bucket = u64::from_str_radix(&digest[..8], 16).unwrap_or(0) % 100;
    if bucket < toggles.file_read_compression_forward_share as u64 {
        ARM_FORWARD
    } else {
        ARM_CONTROL
    }
}

pub(crate) fn effective_forwarding(mode: &str, forward_share: usize) -> Value {
    let (state, summary) = if mode == "forward" {
        (
            "forced_forwarding",
            "Every new session is sent shortened requests; forward mode overrides the share."
                .to_owned(),
        )
    } else if forward_share == 0 {
        (
            "pure_shadow",
            "No session is sent shortened requests; the compressor only records what it would cut."
                .to_owned(),
        )
    } else {
        (
            "sampled_forwarding",
            format!(
                "{forward_share}% of new sessions (by session id) are sent shortened requests; the rest are the shadow control."
            ),
        )
    };
    serde_json::json!({
        "state": state,
        "mode": mode,
        "forward_share": forward_share,
        "summary": summary,
        "applies": "new_sessions",
        "session_arm": "each request's manifest records its session's arm in the file_read_compression stage's resolved_parameters.arm",
    })
}

fn with_arm(mut record: ShadowRecord, arm: &str) -> ShadowRecord {
    if let Some(map) = record.parameters.as_object_mut() {
        map.insert("arm".to_string(), Value::String(arm.to_string()));
    }
    record
}

pub(crate) const GATE_REDUCER: &str = "file_read";

fn open_gate(raw: &[u8]) -> CacheGate {
    let parsed = serde_json::from_slice(raw).unwrap_or(Value::Null);
    CacheGate::open(GATE_REDUCER, raw, &parsed)
}

fn applied_ids(targets: &[TargetEntry]) -> HashSet<String> {
    targets
        .iter()
        .filter(|t| t.outcome == StageOutcome::Applied)
        .filter_map(|t| t.tool_use_id.clone())
        .collect()
}

fn forward(body: &mut StageBody, reduction: file_reads::FileReadReduction, original_bytes: usize) {
    let candidate = reduction.candidate;
    let candidate_bytes = candidate.len();
    let record = with_arm(
        shadow_record(reduction.stage, original_bytes, candidate_bytes),
        ARM_FORWARD,
    );
    let shortened = record.parameters["would_reduce"].clone();
    body.rewrite_forwarded_only(
        candidate.clone(),
        AppliedStage {
            stage: STAGE,
            policy_id: STAGE,
            policy_version: 4,
            resolved_parameters: record.parameters,
            new_bytes: &candidate,
            targets: record.targets,
            reason_code: FORWARDED,
            detail: Some(format!(
                "forwarded: {shortened} read result(s) shortened; the client's original body is kept as the request body"
            )),
        },
    );
}

#[async_trait::async_trait]
impl RequestStage for FileReadCompressionStage {
    fn name(&self) -> &'static str {
        STAGE
    }

    fn enabled(&self, ctx: &StageContext<'_>) -> bool {
        ctx.session_toggles.file_read_compression
    }

    async fn apply(&self, ctx: &StageContext<'_>, body: &mut StageBody) {
        let toggles = ctx.session_toggles;
        let arm = session_arm(toggles, ctx.session_id);
        let gate = (arm == ARM_FORWARD).then(|| open_gate(body.body()));
        let policy = session_policy(toggles)
            .with_capture(body.original())
            .with_warm_cuts(gate.as_ref().and_then(CacheGate::warm_cuts));
        let reduction = file_reads::reduce_file_reads(body.body().as_ref(), &policy);
        if let Some(gate) = gate {
            gate.close(applied_ids(&reduction.stage.targets));
        }
        let original_bytes = body.body().len();
        let candidate_bytes = reduction.candidate.len();
        let candidate_found = reduction.stage.outcome == StageOutcome::Applied;
        if candidate_found {
            tracing::debug!(
                request_id = %ctx.request_id,
                session_id = %ctx.session_id,
                targets = reduction.stage.targets.len(),
                original_bytes,
                candidate_bytes,
                arm,
                "file-read compression: reduction computed"
            );
        }
        if candidate_found && arm == ARM_FORWARD {
            forward(body, reduction, original_bytes);
            return;
        }
        let stage_reason = reduction.stage.reason_code.clone();
        let reason_code = if candidate_found {
            SHADOW_CANDIDATE
        } else {
            stage_reason.as_str()
        };
        let record = with_arm(
            shadow_record(reduction.stage, original_bytes, candidate_bytes),
            arm,
        );
        body.decline_with(UnchangedStage {
            stage: STAGE,
            policy_id: STAGE,
            reason_code,
            resolved_parameters: record.parameters,
            targets: record.targets,
            detail: record.detail,
        });
    }
}

#[cfg(test)]
#[path = "stage_tests.rs"]
mod tests;
