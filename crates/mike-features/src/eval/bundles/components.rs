use serde_json::{Value, json};

use super::COMPONENTS_VERSION;
use super::fingerprint;
use crate::analysis::judge::{JUDGE_RUBRIC_VERSION, judge_verdict_schema};
use crate::config::session_judge::SessionJudgeConfig;
use crate::config::steerer::JudgeConfig;
use crate::judge_worker::session_poll_digest::{
    RELATED_SESSION_LEARNING_MAX_CHARS, RELATED_SESSION_OVERVIEW_MAX_CHARS,
    RELATED_SESSIONS_BLOCK_MAX_CHARS, judge_verdict_schema as session_verdict_schema,
};
use crate::judge_worker::{resolve_session_judge_runtime, resolve_turn_judge_runtime};
use crate::settings::{
    SettingsOverlay, resolve_judge_worker_llm_settings, resolve_session_judge_llm_settings,
    resolve_steerer_llm_settings,
};
use crate::steerer::registry::Steerer;
use crate::steerer::verdict::steerer_verdict_schema;

pub fn turn_judge_components(
    overlay: &SettingsOverlay,
    cfg: &JudgeConfig,
    fallback_model: &str,
) -> Value {
    let runtime = resolve_turn_judge_runtime(overlay, cfg, fallback_model);
    let llm = resolve_judge_worker_llm_settings(overlay);
    json!({
        "components_version": COMPONENTS_VERSION,
        "model": runtime.model,
        "system_prompt": runtime.system_prompt,
        "confidence_threshold": runtime.confidence_threshold,
        "reasoning_effort": llm.reasoning_effort,
        "max_output_tokens": llm.max_output_tokens,
        "rubric_version": JUDGE_RUBRIC_VERSION,
        "output_schema": judge_verdict_schema(),
        "input_builder": {"probe_sha256": fingerprint::turn_judge_probe()},
    })
}

pub fn session_judge_components(overlay: &SettingsOverlay, cfg: &SessionJudgeConfig) -> Value {
    let runtime = resolve_session_judge_runtime(overlay, cfg);
    let llm = resolve_session_judge_llm_settings(overlay);
    json!({
        "components_version": COMPONENTS_VERSION,
        "model": runtime.model,
        "system_prompt": runtime.system_prompt,
        "max_turns_in_prompt": runtime.max_turns_in_prompt,
        "max_chars_per_turn": runtime.max_chars_per_turn,
        "reasoning_effort": llm.reasoning_effort,
        "max_output_tokens": llm.max_output_tokens,
        "rubric_version": cfg.rubric_version,
        "output_schema": session_verdict_schema(),
        "related_limits": {
            "overview_max_chars": RELATED_SESSION_OVERVIEW_MAX_CHARS,
            "learning_max_chars": RELATED_SESSION_LEARNING_MAX_CHARS,
            "block_max_chars": RELATED_SESSIONS_BLOCK_MAX_CHARS,
        },
        "input_builder": {"probe_sha256": fingerprint::session_judge_probe()},
    })
}

pub struct SteererPrompt {
    pub text: String,
    pub source: String,
}

pub struct SteererComponentInputs<'a> {
    pub steerer: &'a Steerer,
    pub prompt: &'a SteererPrompt,
    pub fleet_default_model: &'a str,
    pub overlay: &'a SettingsOverlay,
    pub extras: Option<Value>,
}

fn gate_block(steerer: &Steerer) -> Value {
    if steerer.invocation == "every_turn" {
        return Value::Null;
    }
    json!({
        "probe_sha256": fingerprint::gate_probe(steerer.pattern.as_deref().unwrap_or_default(), &steerer.kind),
    })
}

pub fn steerer_components(inputs: &SteererComponentInputs<'_>) -> Value {
    let steerer = inputs.steerer;
    let llm = resolve_steerer_llm_settings(inputs.overlay);
    let own_model = if steerer.model.is_empty() {
        inputs.fleet_default_model
    } else {
        steerer.model.as_str()
    };
    let model_effective = inputs
        .overlay
        .llm_provider
        .resolve(Some(crate::llm::provider::ModelTier::Strong), own_model);
    let mut components = json!({
        "components_version": COMPONENTS_VERSION,
        "steerer_id": steerer.id,
        "name": steerer.name,
        "kind": steerer.kind,
        "enabled": steerer.enabled,
        "authority": steerer.authority,
        "mode": steerer.mode,
        "trigger_point": steerer.trigger_point,
        "invocation": steerer.invocation,
        "pattern": steerer.pattern,
        "project_id": steerer.project_id,
        "params": {
            "confidence_threshold": steerer.params.confidence_threshold,
            "debounce_turns": steerer.params.debounce_turns,
            "arming_window_turns": steerer.params.arming_window_turns,
            "suppress_first_n_turns": steerer.params.suppress_first_n_turns,
        },
        "model_effective": model_effective,
        "system_prompt": inputs.prompt.text,
        "prompt_source": inputs.prompt.source,
        "output_schema": steerer_verdict_schema(),
        "reasoning_effort": llm.reasoning_effort,
        "max_output_tokens": llm.max_output_tokens,
        "input_builder": {
            "probe_sha256": fingerprint::steerer_probe(
                &steerer.kind,
                &steerer.trigger_point,
                &inputs.overlay.steerer_verifier_input_budget,
            ),
        },
    });
    let gate = gate_block(steerer);
    if let Some(map) = components.as_object_mut() {
        if !gate.is_null() {
            map.insert("gate".to_owned(), gate);
        }
        if steerer.kind == "verifier" {
            map.insert(
                "verifier_input_budget".to_owned(),
                json!(inputs.overlay.steerer_verifier_input_budget),
            );
        }
        if let Some(extras) = &inputs.extras {
            map.insert("calibration".to_owned(), extras.clone());
        }
    }
    components
}
