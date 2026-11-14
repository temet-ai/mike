use super::*;
use crate::lesson_cache::marks::LessonOutcome;

#[test]
fn distinctive_task_terms_outweigh_repeated_project_boilerplate() {
    let snapshot = LessonSnapshot::from_pairs([(
        "/repo".into(),
        vec![
            ("convention".into(), "project checks alpha".into()),
            ("convention".into(), "project checks beta".into()),
            ("convention".into(), "project checks gamma".into()),
            ("convention".into(), "project checks delta".into()),
            ("correction".into(), "cache invalidation".into()),
        ],
    )]);
    let candidates: Vec<_> = snapshot.for_project("/repo").iter().collect();
    let selected = snapshot
        .relevance
        .select(&candidates, "session", "project checks cache", 1);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].text, "cache invalidation");
}

#[test]
fn lexical_matching_beats_recency_and_delivery_is_versioned() {
    let snapshot = LessonSnapshot::from_pairs([(
        "project".into(),
        vec![
            ("correction".into(), "Use blue backgrounds".into()),
            (
                "correction".into(),
                "Reuse cargo test results before running cargo again".into(),
            ),
        ],
    )]);
    let cache = LessonRelevance::default();
    let selected = cache.select(
        &snapshot.for_project("project").iter().collect::<Vec<_>>(),
        "session",
        "cargo test",
        1,
    );
    assert!(selected[0].text.contains("cargo"));
    cache.delivered("session", &selected);
    let next = cache.select(
        &snapshot.for_project("project").iter().collect::<Vec<_>>(),
        "session",
        "cargo test",
        1,
    );
    assert!(next.is_empty());
    assert_eq!(
        cache.select(
            &snapshot.for_project("project").iter().collect::<Vec<_>>(),
            "other",
            "cargo test",
            1
        )[0]
        .id,
        selected[0].id
    );
}

#[tokio::test]
async fn cached_semantics_retrieves_without_word_overlap() {
    let snapshot = LessonSnapshot::from_pairs([(
        "project".into(),
        vec![
            ("correction".into(), "Blue backgrounds".into()),
            ("correction".into(), "Reuse verification evidence".into()),
        ],
    )]);
    let cache = LessonRelevance::default();
    {
        let mut state = cache.state.lock().await;
        state
            .queries
            .insert(fingerprint("avoid duplicate checks"), vec![1.0, 0.0]);
        state.documents.insert(2, vec![1.0, 0.0]);
    }
    assert_eq!(
        cache.select(
            &snapshot.for_project("project").iter().collect::<Vec<_>>(),
            "session",
            "avoid duplicate checks",
            1
        )[0]
        .id,
        2
    );
}

#[test]
fn pending_queries_are_bounded_and_repeated_queries_are_coalesced() {
    let mut state = State::default();
    for n in 0..256 {
        queue_query(&mut state, "session", &n.to_string(), "query");
    }
    queue_query(&mut state, "session", "1", "query");
    assert_eq!(state.pending.len(), QUERY_CAPACITY);
}

#[test]
fn delivered_lesson_returns_only_after_resend_window_and_long_queries_still_queue() {
    let snapshot = LessonSnapshot::from_pairs([(
        "project".into(),
        vec![("correction".into(), "Reuse cargo results".into())],
    )]);
    let cache = LessonRelevance::default();
    let candidates = snapshot.for_project("project").iter().collect::<Vec<_>>();
    cache.observe_request("session");
    let selected = cache.select(&candidates, "session", "cargo", 1);
    cache.delivered("session", &selected);
    for _ in 1..RESEND_AFTER_REQUESTS {
        cache.observe_request("session");
        assert!(cache.select(&candidates, "session", "cargo", 1).is_empty());
    }
    cache.observe_request("session");
    assert_eq!(cache.select(&candidates, "session", "cargo", 1).len(), 1);
    let mut state = State::default();
    queue_query(
        &mut state,
        "session",
        "long",
        &"current context ".repeat(2000),
    );
    assert_eq!(state.pending.len(), 1);
    assert!(state.pending[0].2.len() < 9000);
}

#[test]
fn supply_floor_weak_matches_supply_nothing_and_drop_far_below_best() {
    let snapshot = LessonSnapshot::from_pairs([(
        "project".into(),
        vec![
            ("correction".into(), "cache invalidation".into()),
            (
                "correction".into(),
                format!("cache {}", "unrelated filler words here ".repeat(20)),
            ),
        ],
    )]);
    let candidates = snapshot.for_project("project").iter().collect::<Vec<_>>();
    let cache = LessonRelevance::default();
    let chosen = cache.select(&candidates, "session", "cache invalidation", 5);
    assert_eq!(chosen.len(), 1);
    assert_eq!(chosen[0].text, "cache invalidation");
    let weak = cache.select(
        &candidates[1..],
        "session",
        &format!("cache {}", "other distinct terms ".repeat(30)),
        5,
    );
    assert!(weak.is_empty());
}

fn outcome(lesson_id: i64, judged: i64, used: i64) -> LessonOutcome {
    LessonOutcome {
        lesson_id,
        judged,
        used,
        corrected_anyway: 0,
        ignored: judged - used,
    }
}

async fn first_chosen(followed: i64, ignored: i64) -> anyhow::Result<i64> {
    let snapshot = LessonSnapshot::from_pairs([(
        "project".into(),
        vec![
            ("correction".into(), "Run cargo test alpha".into()),
            ("correction".into(), "Run cargo test beta".into()),
        ],
    )]);
    let candidates = snapshot.for_project("project").iter().collect::<Vec<_>>();
    let cache = LessonRelevance::default();
    cache
        .replace_outcomes(vec![outcome(followed, 12, 11), outcome(ignored, 12, 0)])
        .await;
    let chosen = cache.select(&candidates, "session", "cargo test", 1);
    let Some(first) = chosen.first() else {
        anyhow::bail!("nothing chosen for an equally matching pair");
    };
    Ok(first.id)
}

#[tokio::test]
async fn supply_order_equal_match_follows_outcome_record() -> anyhow::Result<()> {
    assert_eq!(first_chosen(1, 2).await?, 1);
    assert_eq!(first_chosen(2, 1).await?, 2);
    Ok(())
}

#[test]
fn outcome_weight_without_evidence_is_neutral_and_ignored_lessons_sink() {
    let outcomes = outcome::Outcomes::default();
    assert!((outcomes.weight(7) - 1.0).abs() < f32::EPSILON);
    assert!(outcome::factor(&outcome(7, 20, 0)) < 0.1);
    assert!(outcome::factor(&outcome(7, 20, 20)) > 1.9);
    assert!((outcome::factor(&outcome(7, 0, 0)) - 1.0).abs() < f32::EPSILON);
}
