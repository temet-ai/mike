use chrono::Utc;

use super::cluster::{GroupRules, Member, mutual_groups};
use super::drift::{DayInputs, DriftStatus, centroid_distance, fires, score_day, z_score};
use super::retrieve::{
    Audience, QueryError, QuerySubject, RetrievedKind, Scope, SemanticQuery, validate,
};
use super::tagger::{AnchorHit, CorrectionRef, assign};
use crate::dossier::view::types::{AsOf, AsOfMode};

fn query(audience: Audience, kinds: Vec<RetrievedKind>, mode: AsOfMode) -> SemanticQuery {
    SemanticQuery {
        subject: QuerySubject::ClosedTurn(1),
        audience,
        kinds,
        scope: Scope::Operator,
        as_of: AsOf {
            at: Utc::now(),
            mode,
        },
        k: 3,
        offset: 0,
    }
}

#[test]
fn judge_audience_rejects_lesson_and_directive_kinds() {
    let q = query(
        Audience::Judge,
        vec![RetrievedKind::Lesson],
        AsOfMode::Knowledge,
    );
    assert!(matches!(
        validate(&q, true),
        Err(QueryError::KindNotAllowed {
            audience: "judge",
            kind: "lesson"
        })
    ));
    let q = query(
        Audience::Judge,
        vec![RetrievedKind::Correction, RetrievedKind::Exchange],
        AsOfMode::Knowledge,
    );
    assert!(validate(&q, true).is_ok());
}

#[test]
fn label_free_audience_rejects_corrections_and_internal_allows_only_them() {
    assert!(
        validate(
            &query(
                Audience::LabelFree,
                vec![RetrievedKind::Correction],
                AsOfMode::Knowledge
            ),
            false
        )
        .is_err()
    );
    assert!(
        validate(
            &query(
                Audience::Internal,
                vec![RetrievedKind::Correction],
                AsOfMode::Knowledge
            ),
            false
        )
        .is_ok()
    );
    assert!(
        validate(
            &query(
                Audience::Internal,
                vec![RetrievedKind::Exchange],
                AsOfMode::Knowledge
            ),
            false
        )
        .is_err()
    );
}

#[test]
fn reconstructed_mode_is_refused_for_live_consumers() {
    let q = query(
        Audience::Judge,
        vec![RetrievedKind::Correction],
        AsOfMode::Reconstructed,
    );
    assert!(matches!(
        validate(&q, true),
        Err(QueryError::ReconstructedForLive(_))
    ));
    assert!(validate(&q, false).is_ok());
}

fn hit(id: i64, pattern: &str, session: &str, request: Option<i64>, score: f64) -> AnchorHit {
    AnchorHit {
        item_id: id,
        pattern: pattern.to_owned(),
        origin_session_uuid: Some(session.to_owned()),
        request_id: request,
        similarity: score + 0.8,
        score,
    }
}

#[test]
fn tagger_abstains_below_min_score_and_records_runner_up_and_margin() {
    let me = CorrectionRef {
        session_uuid: "s0",
        request_id: Some(9),
    };
    let hits = vec![
        hit(1, "empty-turn", "s1", Some(1), 0.08),
        hit(2, "false-done", "s2", Some(2), 0.02),
    ];
    let assigned = assign(hits.clone(), &me, 0.05);
    assert_eq!(
        assigned.as_ref().and_then(|a| a.pattern.as_deref()),
        Some("empty-turn")
    );
    assert_eq!(
        assigned
            .as_ref()
            .and_then(|a| a.runner_up_pattern.as_deref()),
        Some("false-done")
    );
    assert!(
        assigned
            .and_then(|a| a.margin)
            .is_some_and(|m| (m - 0.06).abs() < 1e-9)
    );
    assert_eq!(assign(hits, &me, 0.10).and_then(|a| a.pattern), None);
}

#[test]
fn tagger_never_uses_same_session_or_self_anchor() {
    let me = CorrectionRef {
        session_uuid: "s0",
        request_id: Some(9),
    };
    let hits = vec![
        hit(1, "same-session", "s0", Some(5), 0.9),
        hit(2, "self", "s3", Some(9), 0.9),
        hit(3, "other", "s4", Some(7), 0.06),
    ];
    let a = assign(hits, &me, 0.05);
    assert_eq!(a.and_then(|a| a.pattern).as_deref(), Some("other"));
}

fn member(id: i64, session: i64, v: Vec<f32>) -> Member {
    Member {
        item_id: id,
        session_id: session,
        vector: v,
    }
}

#[test]
fn mutual_edges_do_not_chain_through_a_hub() {
    let mut members = vec![member(0, 0, vec![1.0, 1.0, 1.0, 1.0])];
    for i in 1..=30 {
        let mut v = vec![0.0_f32; 4];
        v[usize::try_from(i % 4).unwrap_or(0)] = 1.0;
        v[usize::try_from((i + 1) % 4).unwrap_or(0)] = 0.05 * i as f32;
        members.push(member(i, i, v));
    }
    let rules = GroupRules {
        floor: 0.999,
        min_members: 4,
        min_sessions: 3,
    };
    assert!(
        mutual_groups(&members, rules)
            .iter()
            .all(|g| !g.member_item_ids.contains(&0))
    );
}

#[test]
fn group_requires_min_members_and_sessions_and_picks_the_medoid() {
    let tight = |id, s| member(id, s, vec![1.0, 0.01 * id as f32, 0.0]);
    let members = vec![
        tight(1, 1),
        tight(2, 2),
        tight(3, 3),
        tight(4, 3),
        member(9, 9, vec![0.0, 0.0, 1.0]),
    ];
    let groups = mutual_groups(
        &members,
        GroupRules {
            floor: 0.9,
            min_members: 4,
            min_sessions: 3,
        },
    );
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].member_item_ids, vec![1, 2, 3, 4]);
    assert!([2, 3].contains(&groups[0].medoid_item_id));
    let strict = mutual_groups(
        &members,
        GroupRules {
            floor: 0.9,
            min_members: 4,
            min_sessions: 4,
        },
    );
    assert!(strict.is_empty());
}

#[test]
fn identical_windows_have_zero_distance_and_z_needs_seven_prior_days() {
    assert!(centroid_distance(&[1.0, 2.0], &[2.0, 4.0]).abs() < 1e-12);
    assert_eq!(z_score(0.1, &[0.1; 6]).0, None);
    let (z, mean, _) = z_score(0.2, &[0.1, 0.12, 0.08, 0.1, 0.11, 0.09, 0.1]);
    assert!(z.is_some_and(|z| z > 3.0));
    assert!(mean.is_some());
}

#[test]
fn new_embedder_key_starts_warming_and_drift_fires_only_above_z_and_count() {
    let prior = [0.1, 0.12, 0.08, 0.1, 0.11, 0.09, 0.1];
    let day = |age, n| DayInputs {
        current_sum: Some(&[0.0, 1.0]),
        baseline_sum: Some(&[1.0, 0.0]),
        n_current: n,
        min_current: 30,
        prior: &prior,
        embedder_age_days: age,
    };
    assert_eq!(score_day(&day(3, 40)).status, DriftStatus::Warming);
    assert_eq!(score_day(&day(90, 5)).status, DriftStatus::TooFewItems);
    let scored = score_day(&day(90, 40));
    assert_eq!(scored.status, DriftStatus::Scored);
    assert!(fires(&scored, 3.0));
    assert!(!fires(&scored, 1_000.0));
}
