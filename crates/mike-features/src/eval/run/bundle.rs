use crate::store::persistence::eval_repo::EvalPersist as _;
use anyhow::{Context, Result, ensure};

use super::items::{RunJudge, judge_from_components};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_bundles::BundleRow;

fn bundle_judge(bundle_id: i64, bundle: Option<&BundleRow>) -> Result<RunJudge> {
    let bundle = bundle.with_context(|| format!("no bundle {bundle_id}"))?;
    ensure!(
        bundle.role == "turn_judge",
        "bundle {bundle_id} has role {:?}; expected turn_judge",
        bundle.role
    );
    let judge = judge_from_components(&bundle.components).with_context(|| {
        format!("bundle {bundle_id} does not describe a turn judge this build can run")
    })?;
    let executable = judge.components();
    for (key, value) in executable
        .as_object()
        .context("judge components must be an object")?
    {
        ensure!(
            bundle.components.get(key) == Some(value),
            "bundle {bundle_id} has unsupported or inconsistent {key}"
        );
    }
    if judge.is_r3() {
        if let Some(schema) = bundle.components.get("output_schema") {
            ensure!(
                schema == &crate::analysis::judge::judge_verdict_schema(),
                "bundle {bundle_id} has an unsupported output_schema"
            );
        }
        if let Some(slices) = bundle.components.get("slices") {
            ensure!(
                slices == &serde_json::json!([]),
                "bundle {bundle_id} has unsupported slices"
            );
        }
    }
    Ok(judge)
}

pub async fn load_bundle_judge(persist: &PersistHandle, bundle_id: i64) -> Result<RunJudge> {
    let bundle = persist
        .eval()
        .get_bundle(bundle_id)
        .await
        .with_context(|| format!("failed to load bundle {bundle_id}"))?;
    bundle_judge(bundle_id, bundle.as_ref())
}

#[cfg(test)]
mod tests {
    use super::bundle_judge;
    use crate::analysis::judge::input_v2::INPUT_BUILDER_V2;
    use crate::analysis::judge::rubric::{RubricFamily, V2_2026_09_23, effective_rubric, latest};
    use crate::eval::run::items::RunJudge;
    use crate::judge_worker::v2_judge::V2JudgeConfig;
    use crate::llm::LlmRuntimeSettings;
    use crate::store::persistence::eval_bundles::BundleRow;

    fn fixture() -> (BundleRow, RunJudge) {
        let judge = RunJudge::V2(V2JudgeConfig {
            rubric: effective_rubric(&V2_2026_09_23, Some("selected immutable prompt")),
            builder: INPUT_BUILDER_V2,
            model: "selected-model".to_owned(),
            llm_settings: LlmRuntimeSettings {
                reasoning_effort: "high".to_owned(),
                max_output_tokens: 987,
                ..LlmRuntimeSettings::default()
            },
        });
        let bundle = BundleRow {
            id: 14,
            role: "turn_judge".to_owned(),
            subject_key: String::new(),
            origin: "candidate".to_owned(),
            status: "live_ungated".to_owned(),
            components: judge.components(),
            content_sha256: String::new(),
            build_sha: String::new(),
            build_dirty: false,
            cause: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            superseded_at: None,
            superseded_by_id: None,
            gate_run_id: None,
            promoted_at: None,
            override_reason: None,
        };
        (bundle, judge)
    }

    #[test]
    fn selected_bundle_uses_all_recorded_judge_components() -> anyhow::Result<()> {
        let (bundle, expected) = fixture();
        assert_eq!(bundle_judge(14, Some(&bundle))?, expected);
        Ok(())
    }

    #[test]
    fn selected_bundle_missing_id_is_an_error() {
        let result = bundle_judge(14, None);
        assert!(result.is_err_and(|error| error.to_string().contains("no bundle 14")));
    }

    #[test]
    fn selected_bundle_wrong_role_is_an_error() {
        let (mut bundle, _) = fixture();
        bundle.role = "session_judge".to_owned();
        let result = bundle_judge(14, Some(&bundle));
        assert!(result.is_err_and(|error| error.to_string().contains("expected turn_judge")));
    }

    #[test]
    fn selected_bundle_inconsistent_prompt_is_an_error() {
        let (mut bundle, _) = fixture();
        bundle.components["system_prompt"] = serde_json::json!("tampered prompt");
        assert!(bundle_judge(14, Some(&bundle)).is_err());
    }

    #[test]
    fn selected_bundle_unsupported_version_and_schema_are_errors() {
        for key in ["components_version", "output_schema"] {
            let (mut bundle, _) = fixture();
            bundle.components[key] = serde_json::json!(999);
            assert!(bundle_judge(14, Some(&bundle)).is_err(), "{key}");
        }
    }

    #[test]
    fn selected_legacy_bundle_rejects_components_the_decoder_would_ignore() {
        let (mut bundle, _) = fixture();
        let judge = RunJudge::R3 {
            rubric: effective_rubric(latest(RubricFamily::LegacyOutcome), None),
            model: "selected-model".to_owned(),
            llm_settings: LlmRuntimeSettings::default(),
        };
        for (key, value) in [
            ("system_prompt", serde_json::json!("tampered prompt")),
            ("input_builder", serde_json::json!("unknown-builder")),
            ("components_version", serde_json::json!(999)),
            ("output_schema", serde_json::json!({"type": "number"})),
            ("slices", serde_json::json!(["turnfacts"])),
        ] {
            bundle.components = judge.components();
            bundle.components[key] = value;
            assert!(bundle_judge(14, Some(&bundle)).is_err(), "{key}");
        }
    }

    #[test]
    fn selected_legacy_bundle_uses_supported_recorded_components() -> anyhow::Result<()> {
        let (mut bundle, _) = fixture();
        let judge = RunJudge::R3 {
            rubric: effective_rubric(latest(RubricFamily::LegacyOutcome), None),
            model: "selected-model".to_owned(),
            llm_settings: LlmRuntimeSettings::default(),
        };
        bundle.components = judge.components();
        assert_eq!(bundle_judge(14, Some(&bundle))?, judge);
        Ok(())
    }
}
