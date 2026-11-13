use bytes::Bytes;
use serde_json::Value;

use crate::compose_prompt::eligibility::should_classify;
use crate::compose_prompt::{ComposeOutcome, PromptClassKey};
use crate::extract::request::TurnOrigin;
use crate::metrics::{COUNTER_COMPOSITIONS_CLASS_TOTAL, COUNTER_COMPOSITIONS_TOTAL};
use crate::state::AppState;
use crate::store::models::RequestId;
use crate::wire::manifest::{AppliedStage, ManifestBuilder};

pub struct ComposeResult {
    pub upstream_body: Bytes,
    pub observed_prompt_sha256: Option<String>,
    pub observed_composition_id: Option<i64>,
    pub observed_composition_applied: bool,
    pub observed_unknown_blocks: Option<Value>,
    pub composed_system_text: Option<String>,
}

pub struct TurnIdentity<'a> {
    pub turn_origin: &'a TurnOrigin,
    pub class_key: Option<PromptClassKey>,
    pub is_count_tokens_probe: bool,
    pub mutation_exempt: bool,
}

struct ComposeArmOutcome {
    upstream_body: Bytes,
    prompt_sha256: Option<String>,
    composition_id: Option<i64>,
    composition_applied: bool,
    unknown_blocks: Option<Value>,
}

pub async fn run(
    state: &AppState,
    post_filter_body: Bytes,
    pre_filter_body: &Bytes,
    identity: TurnIdentity<'_>,
    request_id: RequestId,
    manifest: &mut ManifestBuilder,
) -> ComposeResult {
    let TurnIdentity {
        turn_origin,
        class_key,
        is_count_tokens_probe,
        mutation_exempt,
    } = identity;
    let will_classify = should_classify(turn_origin, is_count_tokens_probe, mutation_exempt);
    let compose_outcome = if will_classify {
        state
            .composer
            .classify_and_compose(&post_filter_body, pre_filter_body, class_key.as_ref())
    } else {
        ComposeOutcome::Passthrough {
            prompt_sha256: None,
        }
    };

    let non_classify_identity = if will_classify || !turn_origin.stamps_capture_identity() {
        None
    } else {
        state.composer.observe_prompt_identity(pre_filter_body)
    };

    let mut composed_system_text: Option<String> = None;
    let arm = match compose_outcome {
        ComposeOutcome::Passthrough { prompt_sha256 } => {
            handle_passthrough(prompt_sha256, will_classify, manifest, post_filter_body)
        }
        ComposeOutcome::Templated {
            prompt_sha256,
            composition_id,
            label,
            new_body,
        } => {
            let (arm, text) = handle_templated(
                prompt_sha256,
                composition_id,
                label,
                new_body,
                request_id,
                manifest,
            );
            composed_system_text = text;
            arm
        }
        ComposeOutcome::UnknownShape {
            unknown_blocks,
            prompt_sha256,
        } => handle_unknown_shape(
            unknown_blocks,
            prompt_sha256,
            request_id,
            manifest,
            post_filter_body,
        ),
        ComposeOutcome::ReplaceUnsupported { prompt_sha256 } => {
            handle_replace_unsupported(prompt_sha256, manifest, post_filter_body)
        }
        ComposeOutcome::SpliceTargetMissing {
            prompt_sha256,
            composition_id,
        } => handle_splice_target_missing(
            prompt_sha256,
            composition_id,
            request_id,
            manifest,
            post_filter_body,
        ),
    };

    let mut observed_prompt_sha256 = arm.prompt_sha256;
    if observed_prompt_sha256.is_none() && arm.unknown_blocks.is_none() {
        observed_prompt_sha256 = non_classify_identity;
    }

    ComposeResult {
        upstream_body: arm.upstream_body,
        observed_prompt_sha256,
        observed_composition_id: arm.composition_id,
        observed_composition_applied: arm.composition_applied,
        observed_unknown_blocks: arm.unknown_blocks,
        composed_system_text,
    }
}

fn handle_passthrough(
    prompt_sha256: Option<String>,
    will_classify: bool,
    manifest: &mut ManifestBuilder,
    post_filter_body: Bytes,
) -> ComposeArmOutcome {
    metrics::counter!(
        COUNTER_COMPOSITIONS_TOTAL,
        "outcome" => "passthrough"
    )
    .increment(1);
    if will_classify {
        manifest.record_unchanged("composer", "composer", "composer_passthrough");
    }
    ComposeArmOutcome {
        upstream_body: post_filter_body,
        prompt_sha256,
        composition_id: None,
        composition_applied: false,
        unknown_blocks: None,
    }
}

fn handle_templated(
    prompt_sha256: String,
    composition_id: i64,
    label: String,
    new_body: Bytes,
    request_id: RequestId,
    manifest: &mut ManifestBuilder,
) -> (ComposeArmOutcome, Option<String>) {
    tracing::info!(
        request_id = %request_id,
        prompt_sha256 = %prompt_sha256,
        label = %label,
        "composer rewrote system blocks (template mode)"
    );
    metrics::counter!(
        COUNTER_COMPOSITIONS_TOTAL,
        "outcome" => "templated"
    )
    .increment(1);
    metrics::counter!(
        COUNTER_COMPOSITIONS_CLASS_TOTAL,
        "prompt_sha256" => prompt_sha256.clone(),
        "label" => label.clone(),
    )
    .increment(1);

    let composed_system_text = serde_json::from_slice::<Value>(&new_body)
        .ok()
        .and_then(|v| system_block_text(&v, 2));

    manifest.record_applied(AppliedStage {
        stage: "composer",
        policy_id: "composer",
        policy_version: 1,
        resolved_parameters: serde_json::json!({
            "composition_id": composition_id,
            "label": label,
            "prompt_sha256": prompt_sha256,
        }),
        new_bytes: new_body.as_ref(),
        targets: Vec::new(),
        reason_code: "composition_templated",
        detail: Some(format!("composition_id={composition_id} label={label}")),
    });

    (
        ComposeArmOutcome {
            upstream_body: new_body,
            prompt_sha256: Some(prompt_sha256),
            composition_id: Some(composition_id),
            composition_applied: true,
            unknown_blocks: None,
        },
        composed_system_text,
    )
}

fn handle_unknown_shape(
    unknown_blocks: Value,
    prompt_sha256: Option<String>,
    request_id: RequestId,
    manifest: &mut ManifestBuilder,
    post_filter_body: Bytes,
) -> ComposeArmOutcome {
    tracing::warn!(
        request_id = %request_id,
        "composer saw unknown system shape; passthrough"
    );
    metrics::counter!(
        COUNTER_COMPOSITIONS_TOTAL,
        "outcome" => "unknown_shape"
    )
    .increment(1);
    manifest.record_unchanged("composer", "composer", "composer_unknown_shape");
    let anomaly = if prompt_sha256.is_some() {
        None
    } else {
        Some(unknown_blocks)
    };
    ComposeArmOutcome {
        upstream_body: post_filter_body,
        prompt_sha256,
        composition_id: None,
        composition_applied: false,
        unknown_blocks: anomaly,
    }
}

fn handle_replace_unsupported(
    prompt_sha256: String,
    manifest: &mut ManifestBuilder,
    post_filter_body: Bytes,
) -> ComposeArmOutcome {
    metrics::counter!(
        COUNTER_COMPOSITIONS_TOTAL,
        "outcome" => "replace_unsupported"
    )
    .increment(1);
    manifest.record_unchanged("composer", "composer", "composer_replace_unsupported");
    ComposeArmOutcome {
        upstream_body: post_filter_body,
        prompt_sha256: Some(prompt_sha256),
        composition_id: None,
        composition_applied: false,
        unknown_blocks: None,
    }
}

fn handle_splice_target_missing(
    prompt_sha256: String,
    composition_id: i64,
    request_id: RequestId,
    manifest: &mut ManifestBuilder,
    post_filter_body: Bytes,
) -> ComposeArmOutcome {
    tracing::warn!(
        request_id = %request_id,
        prompt_sha256 = %prompt_sha256,
        composition_id = composition_id,
        "composer matched a class but the post-filter body has no splice target; passthrough"
    );
    metrics::counter!(
        COUNTER_COMPOSITIONS_TOTAL,
        "outcome" => "splice_target_missing"
    )
    .increment(1);
    manifest.record_unchanged("composer", "composer", "composer_splice_target_missing");
    ComposeArmOutcome {
        upstream_body: post_filter_body,
        prompt_sha256: Some(prompt_sha256),
        composition_id: Some(composition_id),
        composition_applied: false,
        unknown_blocks: None,
    }
}

pub fn system_block_text(body: &Value, block_index: usize) -> Option<String> {
    body.get("system")?
        .get(block_index)?
        .get("text")?
        .as_str()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_block_text_canonical_shape_returns_text() {
        let body = serde_json::json!({
            "system": [
                {"type": "text", "text": "block0"},
                {"type": "text", "text": "block1"},
                {"type": "text", "text": "block2 spliced content"},
            ]
        });
        assert_eq!(
            system_block_text(&body, 2),
            Some("block2 spliced content".to_string())
        );
    }

    #[test]
    fn system_block_text_missing_block_returns_none() {
        let body = serde_json::json!({
            "system": [{"type": "text", "text": "only block"}]
        });
        assert_eq!(system_block_text(&body, 2), None);
    }

    #[test]
    fn system_block_text_string_form_system_returns_none() {
        let body = serde_json::json!({"system": "a plain string prompt"});
        assert_eq!(system_block_text(&body, 2), None);
    }

    #[test]
    fn system_block_text_no_system_field_returns_none() {
        let body = serde_json::json!({"messages": []});
        assert_eq!(system_block_text(&body, 2), None);
    }

    #[test]
    fn system_block_text_non_text_block_returns_none() {
        let body = serde_json::json!({
            "system": [
                {"type": "text", "text": "block0"},
                {"type": "text", "text": "block1"},
                {"type": "image", "source": {}},
            ]
        });
        assert_eq!(system_block_text(&body, 2), None);
    }
}
