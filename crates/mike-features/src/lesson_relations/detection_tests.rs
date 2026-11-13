use super::adjudication::run_adjudicated_pair;
use super::detection::detect_one_pair;
use super::rules::{find_artifact_reference, find_resolution_supersession};
use super::schema::{AdjudicatorVerdict, ProposerVerdict};
use super::{KIND_SUPERSEDES, LessonRelationsError, RelationJudge, STATUS_PROPOSED};
use crate::llm::LlmError;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lesson_relations::CandidateLessonPair;

#[test]
fn assert_models_differ_rejects_identical_models() {
    let result = super::assert_models_differ("openai/gpt-5.6-luna", "openai/gpt-5.6-luna");
    assert!(
        matches!(result, Err(LessonRelationsError::SameModel(ref m)) if m == "openai/gpt-5.6-luna"),
        "identical proposer/adjudicator models must be rejected before any LLM call: {result:?}"
    );
}

#[test]
fn assert_models_differ_accepts_distinct_models() {
    let result = super::assert_models_differ("openai/gpt-5.6-luna", "google/gemini-3.5-flash-lite");
    assert!(result.is_ok());
}

const LESSON_20_TEXT: &str = "Cross-session mailbox delivery can land in SUBAGENT requests, not the session's main loop. Subagent/worker requests carry the same session id, and the mailbox reminder path picks the session's next eligible request without filtering by turn origin.";
const LESSON_29_TEXT: &str = "RESOLVED (commit d723bc97 on feature/mailbox-delivery-gaps, 2026-07-03): cross-session mailbox delivery landing on SUBAGENT requests (the request-88420 failure) is FIXED. Root cause: extract_turn_origin did not classify subagent replays.";

struct StubJudge {
    a_id: i64,
    b_id: i64,
}

impl RelationJudge for StubJudge {
    async fn propose(&self, prompt: &str) -> Result<ProposerVerdict, LlmError> {
        let (a_id, b_id) = (self.a_id, self.b_id);
        let a_has_markers = !prompt.contains("A polarity markers: none found");
        let (from_lesson_id, to_lesson_id) = if a_has_markers {
            (a_id, b_id)
        } else {
            (b_id, a_id)
        };
        Ok(ProposerVerdict {
            is_related: true,
            kind: KIND_SUPERSEDES.to_string(),
            from_lesson_id,
            to_lesson_id,
            rationale: "one side records the resolution of the other".to_string(),
            quoted_evidence: vec!["RESOLVED (commit d723bc97".to_string()],
        })
    }

    async fn adjudicate(&self, _prompt: &str) -> Result<AdjudicatorVerdict, LlmError> {
        Ok(AdjudicatorVerdict {
            from_lesson_id: 29,
            to_lesson_id: 20,
            rationale: "the resolution marker names the other lesson's failure".to_string(),
        })
    }

    fn proposer_model(&self) -> &str {
        "stub/proposer"
    }

    fn adjudicator_model(&self) -> &str {
        "stub/adjudicator"
    }
}

fn lessons_20_and_29(distance: f64) -> CandidateLessonPair {
    CandidateLessonPair {
        lesson_a_id: 20,
        lesson_a_text: LESSON_20_TEXT.to_string(),
        lesson_a_created_at: "2026-07-02T19:03:24.43086Z".parse().unwrap_or_default(),
        lesson_b_id: 29,
        lesson_b_text: LESSON_29_TEXT.to_string(),
        lesson_b_created_at: "2026-07-03T11:15:47.276659Z".parse().unwrap_or_default(),
        distance,
    }
}

#[test]
fn find_resolution_supersession_fires_for_29_supersedes_20() {
    let pair = lessons_20_and_29(0.1052);
    assert_eq!(find_resolution_supersession(&pair), Some((29, 20)));
}

#[test]
fn find_resolution_supersession_does_not_fire_when_both_sides_have_markers() {
    let mut pair = lessons_20_and_29(0.1052);
    pair.lesson_a_text = format!("RESOLVED: {}", pair.lesson_a_text);
    assert_eq!(find_resolution_supersession(&pair), None);
}

#[test]
fn find_resolution_supersession_does_not_fire_when_neither_side_has_markers() {
    let mut pair = lessons_20_and_29(0.1052);
    pair.lesson_b_text = LESSON_20_TEXT.to_string();
    assert_eq!(find_resolution_supersession(&pair), None);
}

#[test]
fn find_resolution_supersession_does_not_fire_on_low_subject_overlap() {
    let pair = CandidateLessonPair {
        lesson_a_id: 100,
        lesson_a_text: "Storybook stories should colocate with their component.".to_string(),
        lesson_a_created_at: "2026-07-02T19:03:24.43086Z".parse().unwrap_or_default(),
        lesson_b_id: 101,
        lesson_b_text: "RESOLVED: the deploy script now retries on transient network errors."
            .to_string(),
        lesson_b_created_at: "2026-07-03T11:15:47.276659Z".parse().unwrap_or_default(),
        distance: 0.1,
    };
    assert_eq!(find_resolution_supersession(&pair), None);
}

#[test]
fn find_resolution_supersession_does_not_fire_when_marked_side_is_older() {
    let pair = CandidateLessonPair {
        lesson_a_id: 20,
        lesson_a_text: LESSON_20_TEXT.to_string(),
        lesson_a_created_at: "2026-07-04T00:00:00Z".parse().unwrap_or_default(),
        lesson_b_id: 29,
        lesson_b_text: LESSON_29_TEXT.to_string(),
        lesson_b_created_at: "2026-07-01T00:00:00Z".parse().unwrap_or_default(),
        distance: 0.1052,
    };
    assert_eq!(find_resolution_supersession(&pair), None);
}

#[tokio::test]
async fn lesson_29_supersedes_lesson_20_never_the_reverse() {
    let pair = lessons_20_and_29(0.1052);
    let Some((bundle, proposal, verdict)) =
        run_adjudicated_pair(&StubJudge { a_id: 20, b_id: 29 }, &pair).await
    else {
        panic!("the 29/20 pair must yield a proposal");
    };

    assert_eq!(proposal.kind, KIND_SUPERSEDES);
    assert_eq!(
        (proposal.from_lesson_id, proposal.to_lesson_id),
        (29, 20),
        "29 resolves 20, so 29 supersedes 20 — the reverse is the failure that destroyed the better lesson four times"
    );
    assert_eq!((verdict.from_lesson_id, verdict.to_lesson_id), (29, 20));

    assert!(
        bundle.lesson_a_spans.is_empty(),
        "lesson 20 carries no resolution marker"
    );
    let markers: Vec<_> = bundle.lesson_b_spans.iter().map(|s| s.marker).collect();
    assert!(markers.contains(&"RESOLVED") && markers.contains(&"FIXED"));

    for span in &bundle.lesson_b_spans {
        let reextracted: String = LESSON_29_TEXT
            .chars()
            .skip(span.start)
            .take(span.end - span.start)
            .collect();
        assert_eq!(
            reextracted, span.quote,
            "every recorded offset must re-extract its own quote from the stored text"
        );
    }
}

#[tokio::test]
async fn direction_is_unchanged_when_cosine_distance_changes() {
    let Some((_, near, _)) = run_adjudicated_pair(
        &StubJudge { a_id: 20, b_id: 29 },
        &lessons_20_and_29(0.1052),
    )
    .await
    else {
        panic!("near pair must yield a proposal");
    };
    let Some((_, far, _)) = run_adjudicated_pair(
        &StubJudge { a_id: 20, b_id: 29 },
        &lessons_20_and_29(0.4073),
    )
    .await
    else {
        panic!("far pair must yield a proposal");
    };

    assert_eq!(
        (near.from_lesson_id, near.to_lesson_id),
        (far.from_lesson_id, far.to_lesson_id),
        "cosine distance may nominate a pair; it must never decide the direction"
    );
    assert_eq!((far.from_lesson_id, far.to_lesson_id), (29, 20));
}

#[test]
fn find_artifact_reference_matches_lesson_number_citation() {
    let text = "RESOLVED: the bug described in lesson 20 is fixed by this change.";
    let Some((quote, start, end)) = find_artifact_reference(text, 20) else {
        panic!("must match a bare 'lesson 20' citation");
    };
    let reextracted: String = text.chars().skip(start).take(end - start).collect();
    assert_eq!(
        reextracted, quote,
        "citation offsets must index into the source text"
    );
}

#[test]
fn find_artifact_reference_matches_hash_citation() {
    let hit = find_artifact_reference("see #20 for the original report", 20);
    assert!(hit.is_some(), "must match a '#20' citation");
}

#[test]
fn find_artifact_reference_does_not_match_a_different_id() {
    let text = "RESOLVED (commit d723bc97 on feature/mailbox-delivery-gaps, 2026-07-03): \
                cross-session mailbox delivery landing on SUBAGENT requests is FIXED.";
    assert!(
        find_artifact_reference(text, 20).is_none(),
        "a commit SHA must never satisfy an artifact-reference id citation"
    );
}

#[test]
fn find_artifact_reference_avoids_partial_number_match() {
    assert!(find_artifact_reference("see lesson 200 for details", 20).is_none());
}

fn lesson_29_supersedes_20_fixture(distance: f64) -> (StubJudge, CandidateLessonPair) {
    (
        StubJudge { a_id: 20, b_id: 29 },
        lessons_20_and_29(distance),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn detector_decides_29_supersedes_20_from_evidence() -> anyhow::Result<()> {
    let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let (judge, pair) = lesson_29_supersedes_20_fixture(0.1052);

    let Some(detected) = detect_one_pair(&persist, &judge, &pair, true).await? else {
        panic!("a related pair with an agreeing adjudicator must produce a detected relation");
    };
    assert_eq!(detected.kind, KIND_SUPERSEDES);
    assert_eq!(
        detected.from_lesson_id, 29,
        "direction must be 29 supersedes 20 (newer resolves older) — the reverse is the \
         exact historical failure this issue exists to stop"
    );
    assert_eq!(detected.to_lesson_id, 20);
    assert_eq!(
        detected.status, STATUS_PROPOSED,
        "an adjudicated relation must never auto-confirm, even when both proposer and \
         adjudicator agree"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn direction_is_unchanged_when_candidate_distance_is_not_the_real_measured_value()
-> anyhow::Result<()> {
    let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let (judge, pair) = lesson_29_supersedes_20_fixture(0.42);

    let Some(detected) = detect_one_pair(&persist, &judge, &pair, true).await? else {
        panic!("a related pair with an agreeing adjudicator must produce a detected relation");
    };
    assert_eq!(
        detected.from_lesson_id, 29,
        "direction must be unchanged by candidate_distance alone"
    );
    assert_eq!(detected.to_lesson_id, 20);
    assert_eq!(detected.kind, KIND_SUPERSEDES);

    db.cleanup().await?;
    Ok(())
}
