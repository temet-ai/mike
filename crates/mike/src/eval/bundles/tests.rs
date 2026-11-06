use serde_json::json;

use super::components::turn_judge_components;
use super::fingerprint::{probe_sha256, session_judge_probe, turn_judge_probe};
use super::*;

#[test]
fn canonical_json_hash_ignores_key_order() {
    let a = json!({"b": 1, "a": {"y": 2, "x": [1, {"q": 1, "p": 2}]}});
    let b = json!({"a": {"x": [1, {"p": 2, "q": 1}], "y": 2}, "b": 1});
    assert_eq!(content_sha256(&a), content_sha256(&b));
    assert_ne!(content_sha256(&a), content_sha256(&json!({"b": 2})));
}

#[test]
fn a_changed_probe_output_changes_the_fingerprint() {
    let before = probe_sha256(&["user prompt\nassistant text".to_owned()]);
    let after = probe_sha256(&["user prompt\nassistant text (capped)".to_owned()]);
    assert_ne!(before, after);
}

#[test]
fn judge_probes_are_deterministic() {
    assert_eq!(turn_judge_probe(), turn_judge_probe());
    assert_eq!(session_judge_probe(), session_judge_probe());
    assert_eq!(turn_judge_probe().len(), 64);
}

#[test]
fn turn_judge_components_carry_the_whole_compiled_prompt() {
    let overlay = crate::settings::SettingsOverlay::default();
    let cfg = crate::steerer::config::JudgeConfig::default();
    let components = turn_judge_components(&overlay, &cfg, "openai/gpt-test");
    assert_eq!(
        components["system_prompt"],
        json!(crate::analysis::judge::judge_system_prompt())
    );
    assert_eq!(components["components_version"], json!(COMPONENTS_VERSION));
    assert!(
        components.get("semantic").is_none(),
        "no semantic slice is approved"
    );
    assert!(components["input_builder"]["probe_sha256"].is_string());
}

#[test]
fn a_model_setting_changes_the_turn_judge_bundle_content() {
    let cfg = crate::steerer::config::JudgeConfig::default();
    let base = crate::settings::SettingsOverlay::default();
    let changed = crate::settings::SettingsOverlay {
        judge_worker_model: Some("openai/other".to_owned()),
        ..Default::default()
    };
    let a = content_sha256(&turn_judge_components(&base, &cfg, "m"));
    let b = content_sha256(&turn_judge_components(&changed, &cfg, "m"));
    assert_ne!(a, b);
}

#[test]
fn steerer_input_probe_sees_the_verifier_budget() {
    let uncapped = fingerprint::steerer_probe("verifier", "turn_closed", "uncapped");
    let capped = fingerprint::steerer_probe("verifier", "turn_closed", "verifier-input-v1");
    assert_ne!(
        uncapped, capped,
        "the 320,000-character fixture must hit the cap"
    );
    let other = fingerprint::steerer_probe("drift", "turn_closed", "verifier-input-v1");
    assert_eq!(
        other,
        fingerprint::steerer_probe("drift", "turn_closed", "uncapped"),
        "a non-verifier steerer ignores the verifier budget"
    );
}

#[test]
fn bundle_ids_look_up_steerers() {
    let mut ids = BundleIds::default();
    ids.steerers.insert(7, 42);
    assert_eq!(ids.steerer(7), Some(42));
    assert_eq!(ids.steerer(8), None);
}

#[test]
fn causes_serialise_their_trigger_and_change_ids() {
    let cause = BundleCause::setting("judge_worker.model", vec![11, 12]);
    let value = cause.to_value();
    assert_eq!(value["trigger"], "setting");
    assert_eq!(value["change_log_ids"], json!([11, 12]));
    assert!(value.get("rollup_id").is_none());
    assert_eq!(
        BundleCause::calibration(5, vec![]).to_value()["rollup_id"],
        5
    );
}
