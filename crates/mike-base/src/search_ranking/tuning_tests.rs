use std::collections::BTreeMap;

use serde_json::json;

use super::tuning::{
    Evaluation, Grade, GradeCandidate, MIN_GAIN, evaluate, reranked_order, tune, verdict,
};
use super::{KEY_KIND_WEIGHTS, KEY_LEXICAL_WEIGHT, RankingConfig};

fn graded(reference: &str, kind: &str, lexical: f64, score: Option<f64>) -> GradeCandidate {
    GradeCandidate {
        reference: reference.to_owned(),
        kind: kind.to_owned(),
        lexical,
        semantic: 0.0,
        rerank_score: score,
    }
}

fn target_last() -> Grade {
    let mut candidates: Vec<GradeCandidate> = (0..6)
        .map(|i| {
            graded(
                &format!("lesson:{i}"),
                "lesson",
                0.9 - 0.1 * f64::from(i),
                None,
            )
        })
        .collect();
    candidates.push(graded("turn:t", "turn", 0.3, None));
    Grade {
        candidates,
        used: vec!["turn:t".to_owned()],
    }
}

fn evaluation(overall: f64, kinds: &[(&str, f64)]) -> Evaluation {
    Evaluation {
        overall,
        by_kind: kinds
            .iter()
            .map(|(kind, share)| ((*kind).to_owned(), *share))
            .collect::<BTreeMap<_, _>>(),
    }
}

#[test]
fn reranked_order_puts_scored_first_then_unscored_then_the_tail() {
    let scores = [Some(0.1), Some(0.9), None, Some(0.5)];

    assert_eq!(
        reranked_order(vec![0, 1, 2, 3], &scores, 3),
        vec![1, 0, 2, 3]
    );
}

#[test]
fn reranked_order_with_equal_scores_keeps_the_incoming_order() {
    let scores = [Some(0.4), Some(0.4), Some(0.4)];

    assert_eq!(reranked_order(vec![2, 0, 1], &scores, 3), vec![2, 0, 1]);
}

#[test]
fn reranked_order_with_a_zero_window_changes_nothing() {
    let scores = [Some(0.1), Some(0.9)];

    assert_eq!(reranked_order(vec![0, 1], &scores, 0), vec![0, 1]);
}

#[test]
fn evaluate_counts_a_used_reference_outside_the_top_five_as_a_miss() {
    let result = evaluate(&[target_last()], &RankingConfig::default(), 0);

    assert_eq!(result.overall, 0.0);
    assert_eq!(result.by_kind.get("turn"), Some(&0.0));
}

#[test]
fn evaluate_with_rerank_scores_lifts_the_used_reference_into_the_top_five() {
    let mut grade = target_last();
    for candidate in &mut grade.candidates {
        candidate.rerank_score = Some(if candidate.reference == "turn:t" {
            0.9
        } else {
            0.1
        });
    }

    let result = evaluate(&[grade.clone()], &RankingConfig::default(), 7);
    let unreranked = evaluate(&[grade], &RankingConfig::default(), 0);

    assert_eq!(result.overall, 1.0);
    assert_eq!(unreranked.overall, 0.0);
}

#[test]
fn evaluate_buckets_a_used_reference_missing_from_the_candidates_by_its_prefix() {
    let mut grade = target_last();
    grade.used.push("note:gone".to_owned());

    let result = evaluate(&[grade], &RankingConfig::default(), 0);

    assert_eq!(result.overall, 0.0);
    assert_eq!(result.by_kind.get("note"), Some(&0.0));
}

#[test]
fn verdict_with_a_gain_under_the_minimum_is_refused() {
    let before = evaluation(0.40, &[("lesson", 0.40)]);
    let after = evaluation(0.40 + MIN_GAIN / 2.0, &[("lesson", 0.45)]);

    let reason = verdict(&before, &after, MIN_GAIN).err().unwrap_or_default();

    assert!(reason.contains("under the 0.01 needed"), "{reason}");
}

#[test]
fn verdict_when_a_kind_drops_is_refused_even_with_a_larger_overall_gain() {
    let before = evaluation(0.40, &[("lesson", 0.50), ("turn", 0.30)]);
    let after = evaluation(0.60, &[("lesson", 0.90), ("turn", 0.20)]);

    let reason = verdict(&before, &after, MIN_GAIN).err().unwrap_or_default();

    assert!(reason.contains("share for turn would drop"), "{reason}");
}

#[test]
fn verdict_with_exactly_the_minimum_gain_and_no_drop_is_kept() {
    let before = evaluation(0.40, &[("lesson", 0.40)]);
    let after = evaluation(0.40 + MIN_GAIN, &[("lesson", 0.41)]);

    assert!(verdict(&before, &after, MIN_GAIN).is_ok());
}

#[test]
fn tune_keeps_the_one_kind_weight_that_brings_the_used_reference_into_the_top_five()
-> anyhow::Result<()> {
    let grades: Vec<Grade> = (0..20).map(|_| target_last()).collect();

    let attempts = tune(&grades, &RankingConfig::default(), 0, MIN_GAIN);

    assert_eq!(attempts.len(), 5);
    let kept: Vec<_> = attempts.iter().filter(|a| a.kept).collect();
    let [attempt] = kept.as_slice() else {
        anyhow::bail!("exactly one change must be kept, got {attempts:?}");
    };
    assert_eq!(attempt.key, KEY_KIND_WEIGHTS);
    assert_eq!(attempt.before, json!({}));
    assert_eq!(attempt.after, json!({"turn": 1.25}));
    assert_eq!(attempt.score_before, 0.0);
    assert_eq!(attempt.score_after, 1.0);
    Ok(())
}

#[test]
fn tune_records_the_best_rejected_value_for_a_setting_that_cannot_help() -> anyhow::Result<()> {
    let grades: Vec<Grade> = (0..20).map(|_| target_last()).collect();

    let attempts = tune(&grades, &RankingConfig::default(), 0, MIN_GAIN);

    let Some(lexical) = attempts.iter().find(|a| a.key == KEY_LEXICAL_WEIGHT) else {
        anyhow::bail!("the lexical weight must be tried: {attempts:?}");
    };
    assert!(!lexical.kept);
    assert_eq!(lexical.score_before, lexical.score_after);
    assert!(lexical.reason.contains("under the 0.01 needed"));
    Ok(())
}

#[test]
fn tune_with_nothing_to_improve_keeps_no_change() {
    let mut grade = target_last();
    grade.used = vec!["lesson:0".to_owned()];
    let grades: Vec<Grade> = (0..20).map(|_| grade.clone()).collect();

    let attempts = tune(&grades, &RankingConfig::default(), 0, MIN_GAIN);

    assert!(attempts.iter().all(|attempt| !attempt.kept));
}

#[test]
fn tune_from_a_config_that_already_ranks_the_used_reference_high_keeps_nothing() {
    let grades: Vec<Grade> = (0..20).map(|_| target_last()).collect();
    let mut start = RankingConfig::default();
    start.apply(KEY_KIND_WEIGHTS, &json!({"turn": 1.5}));

    let attempts = tune(&grades, &start, 0, MIN_GAIN);

    assert!(!attempts.is_empty());
    assert!(
        attempts
            .iter()
            .all(|attempt| !attempt.kept && attempt.score_before == 1.0)
    );
}

#[test]
fn grade_from_stored_reads_candidates_and_refuses_malformed_json() {
    let stored = json!([{"reference": "lesson:a", "kind": "lesson", "lexical": 0.5,
        "semantic": 0.0, "rerank_score": null}]);

    let grade = Grade::from_stored(stored, vec!["lesson:a".to_owned()]);

    assert_eq!(grade.map(|g| g.candidates.len()), Some(1));
    assert!(Grade::from_stored(json!({"not": "an array"}), Vec::new()).is_none());
}

#[test]
fn tune_tries_whatever_kind_the_grades_hold() -> anyhow::Result<()> {
    let mut grade = target_last();
    for candidate in &mut grade.candidates {
        if candidate.kind == "turn" {
            candidate.kind = "question_answer".to_owned();
        }
    }
    let grades: Vec<Grade> = (0..20).map(|_| grade.clone()).collect();

    let attempts = tune(&grades, &RankingConfig::default(), 0, MIN_GAIN);

    let Some(kept) = attempts.iter().find(|a| a.kept) else {
        anyhow::bail!("a change must be kept: {attempts:?}");
    };
    assert_eq!(kept.key, KEY_KIND_WEIGHTS);
    assert_eq!(kept.after, json!({"question_answer": 1.25}));
    Ok(())
}
