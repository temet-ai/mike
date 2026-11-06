use mike::store::persistence::PersistHandle;
use mike::store::persistence::operator_turns::{TurnJudgeDetail, TurnJudgmentStamp};
use mike::store::test_support::{TestDb, seed_project, seed_session_turns, seed_two_sessions};

use crate::store::operator_turns::support::unique;

#[tokio::test]
async fn stamp_turn_judgment_with_detail_writes_all_three_columns() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let project_id = seed_project(handle.pool(), &unique("w-turn-judge-detail")).await?;
    let session = uuid::Uuid::new_v4().to_string();
    let spare = uuid::Uuid::new_v4().to_string();
    seed_two_sessions(handle.pool(), &session, &spare, project_id).await?;
    seed_session_turns(handle.pool(), &session, project_id, 1).await?;

    let candidates = handle.claim_unjudged_turns(10, "r1-2026-06-22").await?;
    assert_eq!(
        candidates.len(),
        1,
        "exactly the one seeded turn must be claimable — this test needs an otherwise-empty \
         unjudged-turn queue, so it keeps acquire_isolated"
    );
    let turn_id = candidates[0].turn_id;

    let detail = TurnJudgeDetail {
        rationale: Some("the turn showed a passing test run before claiming done".to_string()),
        unmet_ask: Some(String::new()),
        evidence_quality: Some("verified".to_string()),
    };
    let updated = handle
        .stamp_turn_judgment(TurnJudgmentStamp {
            turn_id,
            outcome_code: "accepted",
            judge_model: "test-model",
            rubric_version: "r2-2026-08-29",
            detail: Some(detail),
            bundle_id: Some(41),
        })
        .await?;
    assert!(updated, "stamp_turn_judgment must find the unjudged row");

    let (rationale, unmet_ask, evidence_quality): (Option<String>, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT rationale, unmet_ask, evidence_quality
               FROM memory.operator_turns
              WHERE id = $1",
        )
        .bind(turn_id)
        .fetch_one(handle.pool())
        .await?;
    assert_eq!(
        rationale.as_deref(),
        Some("the turn showed a passing test run before claiming done")
    );
    assert_eq!(unmet_ask.as_deref(), Some(""));
    assert_eq!(evidence_quality.as_deref(), Some("verified"));

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn stamp_turn_judgment_with_no_detail_leaves_columns_null() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let project_id = seed_project(handle.pool(), &unique("w-turn-judge-no-detail")).await?;
    let session = uuid::Uuid::new_v4().to_string();
    let spare = uuid::Uuid::new_v4().to_string();
    seed_two_sessions(handle.pool(), &session, &spare, project_id).await?;
    seed_session_turns(handle.pool(), &session, project_id, 1).await?;

    let candidates = handle.claim_unjudged_turns(10, "r1-2026-06-22").await?;
    let turn_id = candidates[0].turn_id;

    handle
        .stamp_turn_judgment(TurnJudgmentStamp {
            turn_id,
            outcome_code: "accepted",
            judge_model: "test-model",
            rubric_version: "r1-2026-06-22",
            detail: None,
            bundle_id: None,
        })
        .await?;

    let (rationale, unmet_ask, evidence_quality): (Option<String>, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT rationale, unmet_ask, evidence_quality
               FROM memory.operator_turns
              WHERE id = $1",
        )
        .bind(turn_id)
        .fetch_one(handle.pool())
        .await?;
    assert_eq!(rationale, None);
    assert_eq!(unmet_ask, None);
    assert_eq!(evidence_quality, None);

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
