#[path = "proposals_queue_tests.rs"]
mod queue;
#[path = "write_time_tests.rs"]
mod write_time;

use chrono::Utc;

use super::proposals::{EvidenceWrite, NewCandidateLesson, ProposalError};
use crate::store::persistence::test_support;
use crate::store::persistence::turn_facts::TurnFactsRow;
use crate::store::persistence::turn_verdicts::{
    TurnVerdictInsert, VerdictCallUsage, VerdictJudgement, VerdictMode, VerdictShape,
};
use crate::store::persistence::{PersistHandle, test_support::TestDbHandle};

const PROJECT: &str = "w-lesson-proposals";
const SESSION: &str = "00000000-0000-4000-8000-0000000000f1";
const OTHER_SESSION: &str = "00000000-0000-4000-8000-0000000000f2";

struct Fixture {
    db: TestDbHandle,
    handle: PersistHandle,
    project_id: i64,
    turn_id: i64,
    verdict_id: i64,
}

async fn seed_correcting_request(
    pool: &sqlx::PgPool,
    session_uuid: &str,
    project_id: i64,
    human_prompt: &str,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id,
             params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
             created_at, completed_at, author_kind_id
         )
         SELECT s.id, $2, rm.id, '/v1/messages', false,
                1, false, rs.id, '{}'::jsonb, $3, $3, rtt.id, rto.id, NOW(), NOW(), 1
           FROM sessions.sessions s, requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE s.session_uuid = $1::text::uuid
            AND rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'
          RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .bind(human_prompt)
    .fetch_one(pool)
    .await?;
    sqlx::query("INSERT INTO prompt_kinds.classifications (request_id,session_id,kind_id) SELECT r.id,r.session_id,k.id FROM requests.requests r CROSS JOIN prompt_kinds.kinds k WHERE r.id=$1 AND k.code='operator'")
        .bind(id).execute(pool).await?;
    Ok(id)
}

async fn seed_turn_facts(
    handle: &PersistHandle,
    turn_id: i64,
    session_id: i64,
    max_request_id: i64,
    next_state: &str,
    next_genuine_request_id: Option<i64>,
) -> anyhow::Result<()> {
    let row = TurnFactsRow {
        operator_turn_id: turn_id,
        session_id,
        builder_version: "test-v1".to_owned(),
        built_at: Utc::now(),
        max_request_id,
        prompt_kind: "genuine".to_owned(),
        prompt_rule: None,
        prompt_kind_at_close: "genuine".to_owned(),
        prompt_rule_at_close: None,
        next_state: next_state.to_owned(),
        next_genuine_request_id,
        next_genuine_turn_id: None,
        next_genuine_at: next_genuine_request_id.map(|_| Utc::now()),
        next_state_at_close: next_state.to_owned(),
        next_genuine_request_id_at_close: next_genuine_request_id,
        skipped_before_next: serde_json::json!({}),
        reply_window_segment_ids: Vec::new(),
        reclassified_at: None,
        text_chars: 0,
        text_cut_at: 0,
        last_text_origin: None,
        text_spans: serde_json::json!([]),
        stop_request_id: None,
        stop_reason: None,
        aborted: false,
        interrupted: false,
        close_cause: "next_prompt".to_owned(),
        checks_run: serde_json::json!([]),
        checks_count: 0,
        checks_failed: 0,
        ended_on: "other".to_owned(),
        ended_on_version: "ended-on-v1".to_owned(),
        session_class: "operator".to_owned(),
        session_class_rule: "test".to_owned(),
    };
    handle.upsert_turn_facts(&[row]).await?;
    Ok(())
}

struct SeededVerdict {
    turn_id: i64,
    verdict_id: i64,
}

async fn seed_verdict(
    handle: &PersistHandle,
    session_uuid: &str,
    project_id: i64,
    confidence: f32,
    next_state: &str,
    resolved: bool,
) -> anyhow::Result<SeededVerdict> {
    let pool = handle.pool();
    test_support::seed_session_turns(pool, session_uuid, project_id, 1).await?;
    let (turn_id,): (i64,) = sqlx::query_as(
        "SELECT t.id FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid ORDER BY t.id DESC LIMIT 1",
    )
    .bind(session_uuid)
    .fetch_one(pool)
    .await?;
    let (session_id, max_request_id): (i64, i64) = sqlx::query_as(
        "SELECT session_id, start_request_id FROM memory.operator_turns WHERE id = $1",
    )
    .bind(turn_id)
    .fetch_one(pool)
    .await?;
    let correcting_message = if resolved {
        Some("that broke the build".to_owned())
    } else {
        None
    };
    let next_genuine_request_id = if resolved {
        Some(
            seed_correcting_request(
                pool,
                session_uuid,
                project_id,
                correcting_message.as_deref().unwrap_or_default(),
            )
            .await?,
        )
    } else {
        None
    };
    seed_turn_facts(
        handle,
        turn_id,
        session_id,
        max_request_id,
        next_state,
        next_genuine_request_id,
    )
    .await?;
    let verdict_id = handle
        .insert_turn_verdict(TurnVerdictInsert {
            turn_id,
            mode: VerdictMode::Live,
            bundle_id: None,
            rubric_version: "test-r1",
            input_builder_version: "test-b1",
            slices: Vec::new(),
            model: "test-model",
            judgement: VerdictJudgement {
                shape: VerdictShape::TwoAxis {
                    reaction: "corrected",
                    delivery: "shipped",
                    failure_tags: None,
                    severity: None,
                },
                confidence,
                rationale: "missed the edge case",
                unmet_ask: "handle the empty list",
                evidence_quality: "verified",
            },
            next_genuine_request_id,
            as_of: Utc::now(),
            input_text: "test input",
            usage: VerdictCallUsage::default(),
        })
        .await?;
    sqlx::query("UPDATE memory.turn_verdicts SET projected = true WHERE id = $1")
        .bind(verdict_id)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE memory.operator_turns SET live_verdict_id = $2 WHERE id = $1")
        .bind(turn_id)
        .bind(verdict_id)
        .execute(pool)
        .await?;
    Ok(SeededVerdict {
        turn_id,
        verdict_id,
    })
}

async fn fixture(
    confidence: f32,
    next_state: &str,
    resolved: bool,
) -> anyhow::Result<Option<Fixture>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_id = test_support::seed_project(handle.pool(), PROJECT).await?;
    test_support::seed_two_sessions(handle.pool(), SESSION, OTHER_SESSION, project_id).await?;
    let seeded = seed_verdict(
        &handle, SESSION, project_id, confidence, next_state, resolved,
    )
    .await?;
    Ok(Some(Fixture {
        db,
        handle,
        project_id,
        turn_id: seeded.turn_id,
        verdict_id: seeded.verdict_id,
    }))
}

fn no_evidence(verdict_id: i64, turn_id: i64, similar: &serde_json::Value) -> EvidenceWrite<'_> {
    EvidenceWrite {
        verdict_id,
        operator_turn_id: turn_id,
        lesson_id: None,
        similar,
        model: "test-model",
        cost_usd: None,
        generation_id: None,
        reason: "test reason",
    }
}

async fn create_test_lesson(fx: &Fixture, text: &str) -> anyhow::Result<i64> {
    let embedding = unit_embedding();
    let lesson_id = fx
        .handle
        .create_candidate_lesson(
            NewCandidateLesson {
                project_id: fx.project_id,
                kind: "correction",
                text,
                source_turn_id: fx.turn_id,
                session_db_id: fx.turn_id,
                request_id: None,
                embedding: &embedding,
                embedding_model: "test-embedder",
                shape: None,
                relations: &[],
                placement: None,
            },
            no_evidence(fx.verdict_id, fx.turn_id, &serde_json::json!([])),
        )
        .await?;
    Ok(lesson_id)
}

fn unit_embedding() -> Vec<f32> {
    let mut v = vec![0.0_f32; 768];
    v[0] = 1.0;
    v
}

#[tokio::test]
async fn selects_a_resolved_corrected_verdict_regardless_of_confidence() -> anyhow::Result<()> {
    let Some(fx) = fixture(0.8, "resolved", true).await? else {
        return Ok(());
    };
    let since = Utc::now() - chrono::Duration::days(1);
    let candidates = fx.handle.select_correction_candidates(since, 10).await?;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].verdict_id, fx.verdict_id);
    assert_eq!(
        candidates[0].correcting_message.as_deref(),
        Some("that broke the build")
    );
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn selects_a_verdict_below_the_confidence_floor_for_downstream_gating() -> anyhow::Result<()>
{
    let Some(fx) = fixture(0.5, "resolved", true).await? else {
        return Ok(());
    };
    let since = Utc::now() - chrono::Duration::days(1);
    let candidates = fx.handle.select_correction_candidates(since, 10).await?;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].verdict_id, fx.verdict_id);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn excludes_a_verdict_with_no_resolved_correction() -> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "pending", false).await? else {
        return Ok(());
    };
    let since = Utc::now() - chrono::Duration::days(1);
    let candidates = fx.handle.select_correction_candidates(since, 10).await?;
    assert!(candidates.is_empty());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_verdict_already_evidenced_is_not_reselected() -> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    fx.handle
        .record_evidence(no_evidence(
            fx.verdict_id,
            fx.turn_id,
            &serde_json::json!([]),
        ))
        .await?;
    let since = Utc::now() - chrono::Duration::days(1);
    let candidates = fx.handle.select_correction_candidates(since, 10).await?;
    assert!(candidates.is_empty());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn creates_a_raw_candidate_lesson_with_its_evidence() -> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    let lesson_id = create_test_lesson(&fx, "always handle the empty list case").await?;
    let (stage, source): (String, String) = sqlx::query_as(
        "SELECT ls.code, lsrc.code FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
           JOIN memory.lesson_sources lsrc ON lsrc.id = l.source_id
          WHERE l.id = $1",
    )
    .bind(lesson_id)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(stage, "raw");
    assert_eq!(source, "judge");
    assert_eq!(fx.handle.count_lesson_evidence(lesson_id).await?, 1);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn finds_an_active_lesson_as_a_neighbour_within_the_distance() -> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    let lesson_id = create_test_lesson(&fx, "always handle the empty list case").await?;
    fx.handle
        .approve_lesson_candidate(lesson_id, None, "operator")
        .await?;
    let found = fx
        .handle
        .find_lesson_neighbours(fx.project_id, &unit_embedding(), 0.25, 5)
        .await?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, lesson_id);
    assert_eq!(found[0].source, "judge");
    let mut far = unit_embedding();
    far[0] = 0.0;
    far[1] = 1.0;
    let none = fx
        .handle
        .find_lesson_neighbours(fx.project_id, &far, 0.25, 5)
        .await?;
    assert!(none.is_empty());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn approve_promotes_a_raw_candidate_to_daily_and_logs_the_decision() -> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    let lesson_id = create_test_lesson(&fx, "always handle the empty list case").await?;
    fx.handle
        .approve_lesson_candidate(lesson_id, Some("handle empty lists explicitly"), "operator")
        .await?;
    let (stage, text): (String, String) = sqlx::query_as(
        "SELECT ls.code, l.text FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id WHERE l.id = $1",
    )
    .bind(lesson_id)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(stage, "daily");
    assert_eq!(text, "handle empty lists explicitly");
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM config.change_log
          WHERE subject_kind = 'lesson_decision' AND subject_key = $1",
    )
    .bind(lesson_id.to_string())
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(count, 1);
    let again = fx
        .handle
        .approve_lesson_candidate(lesson_id, None, "operator")
        .await;
    assert!(matches!(again, Err(ProposalError::NotPending)));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn reject_archives_with_the_operator_reason() -> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    let lesson_id = create_test_lesson(&fx, "task-specific, not a rule").await?;
    fx.handle
        .reject_lesson_candidate(lesson_id, "not_a_correction", Some("one-off typo"))
        .await?;
    let (stage, reason): (String, Option<String>) = sqlx::query_as(
        "SELECT ls.code, ar.code FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
           LEFT JOIN memory.lesson_archived_reasons ar ON ar.id = l.archived_reason_id
          WHERE l.id = $1",
    )
    .bind(lesson_id)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(stage, "archived");
    assert_eq!(reason.as_deref(), Some("rejected_by_operator"));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn auto_promote_fires_once_evidence_reaches_the_threshold() -> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    let lesson_id = create_test_lesson(&fx, "always handle the empty list case").await?;
    assert_eq!(fx.handle.count_lesson_evidence(lesson_id).await?, 1);
    let second = seed_verdict(
        &fx.handle,
        OTHER_SESSION,
        fx.project_id,
        0.9,
        "resolved",
        true,
    )
    .await?;
    fx.handle
        .reinforce_lesson(
            lesson_id,
            no_evidence(second.verdict_id, second.turn_id, &serde_json::json!([])),
        )
        .await?;
    let evidence_count = fx.handle.count_lesson_evidence(lesson_id).await?;
    assert_eq!(evidence_count, 2);
    const AUTO_PROMOTE_MIN_EVIDENCE: i64 = 2;
    if evidence_count >= AUTO_PROMOTE_MIN_EVIDENCE {
        fx.handle
            .approve_lesson_candidate(lesson_id, None, "judge")
            .await?;
    }
    let (stage,): (String,) = sqlx::query_as(
        "SELECT ls.code FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id WHERE l.id = $1",
    )
    .bind(lesson_id)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(stage, "daily");
    fx.db.cleanup().await?;
    Ok(())
}
