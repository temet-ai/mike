//! The rest of proposals_tests's tests.

use super::*;

#[tokio::test]
async fn reject_archives_with_the_operator_reason() -> anyhow::Result<()> {
    let Some(fx) = fixture(0.9, "resolved", true).await? else {
        return Ok(());
    };
    let lesson_id = create_test_lesson(&fx, "task-specific, not a rule").await?;
    fx.handle
        .lessons()
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
    assert_eq!(
        fx.handle.lessons().count_lesson_evidence(lesson_id).await?,
        1
    );
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
        .lessons()
        .reinforce_lesson(
            lesson_id,
            no_evidence(second.verdict_id, second.turn_id, &serde_json::json!([])),
        )
        .await?;
    let evidence_count = fx.handle.lessons().count_lesson_evidence(lesson_id).await?;
    assert_eq!(evidence_count, 2);
    const AUTO_PROMOTE_MIN_EVIDENCE: i64 = 2;
    if evidence_count >= AUTO_PROMOTE_MIN_EVIDENCE {
        fx.handle
            .lessons()
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
