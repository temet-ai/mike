use serde_json::json;

use super::{DEFAULT_MODEL, ProposerReport, ProposerSettings, WriteOutcome};

#[test]
fn settings_fall_back_to_every_default_with_no_rows() {
    let settings = ProposerSettings::from_rows(&[]);
    assert_eq!(settings.effective_model, DEFAULT_MODEL);
    assert!((settings.min_confidence - 0.7).abs() < f32::EPSILON);
    assert_eq!(settings.max_per_day, 40);
    assert_eq!(settings.maintenance_max_per_day, 400);
    assert_eq!(settings.auto_promote_min_evidence, 2);
}

#[test]
fn settings_read_the_maintenance_allowance_apart_from_the_corrections_allowance() {
    let settings = ProposerSettings::from_rows(&[(
        "judge_lessons.maintenance_max_per_day".to_owned(),
        json!(250),
    )]);
    assert_eq!(settings.maintenance_max_per_day, 250);
    assert_eq!(settings.max_per_day, 40);
}

#[test]
fn settings_model_falls_back_to_the_session_judge_model_when_unset() {
    let settings = ProposerSettings::from_rows(&[(
        "session_judge.model".to_owned(),
        json!("openai/gpt-5-mini"),
    )]);
    assert_eq!(settings.effective_model, "openai/gpt-5-mini");
}

#[test]
fn settings_own_model_wins_over_the_session_judge_fallback() {
    let settings = ProposerSettings::from_rows(&[
        ("session_judge.model".to_owned(), json!("openai/gpt-5-mini")),
        (
            "judge_lessons.model".to_owned(),
            json!("google/gemini-3.5-flash-lite"),
        ),
    ]);
    assert_eq!(settings.effective_model, "google/gemini-3.5-flash-lite");
}

#[test]
fn settings_blank_model_override_is_ignored() {
    let settings = ProposerSettings::from_rows(&[
        ("judge_lessons.model".to_owned(), json!("   ")),
        ("session_judge.model".to_owned(), json!("openai/gpt-5-mini")),
    ]);
    assert_eq!(settings.effective_model, "openai/gpt-5-mini");
}

#[test]
fn settings_clamp_out_of_range_numbers() {
    let settings = ProposerSettings::from_rows(&[
        ("judge_lessons.max_per_day".to_owned(), json!(5_000)),
        (
            "judge_lessons.auto_promote_min_evidence".to_owned(),
            json!(-3),
        ),
    ]);
    assert_eq!(settings.max_per_day, 1000);
    assert_eq!(settings.auto_promote_min_evidence, 0);
    let settings = ProposerSettings::from_rows(&[(
        "judge_lessons.maintenance_max_per_day".to_owned(),
        json!(99_999),
    )]);
    assert_eq!(settings.maintenance_max_per_day, 5000);
}

#[test]
fn settings_read_the_shape_limit_and_default_it() {
    assert_eq!(ProposerSettings::from_rows(&[]).max_chars, 500);
    let settings =
        ProposerSettings::from_rows(&[("lessons.shape.max_chars".to_owned(), json!(800))]);
    assert_eq!(settings.max_chars, 800);
}

#[test]
fn report_records_every_outcome_kind() {
    let mut report = ProposerReport::default();
    report.record(WriteOutcome::EvidenceOnly);
    report.record(WriteOutcome::AwaitingEmbedding);
    report.record(WriteOutcome::Created {
        lesson_id: 1,
        promoted: false,
    });
    report.record(WriteOutcome::Created {
        lesson_id: 2,
        promoted: true,
    });
    report.record(WriteOutcome::Reinforced {
        lesson_id: 1,
        promoted: true,
    });
    assert_eq!(report.evidence_only, 1);
    assert_eq!(report.awaiting_embedding, 1);
    assert_eq!(report.created, 2);
    assert_eq!(report.reinforced, 1);
    assert_eq!(report.promoted, 2);
}
