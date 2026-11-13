use chrono::{Duration, NaiveDate, Utc};
use mike::store::persistence::lessons::skill_drafts::DraftTarget;
use mike::store::persistence::lessons::skill_live::{RetiredReason, SkillLiveOutcome};
use mike::store::persistence::lessons_repo::LessonsPersist as _;
use serde_json::json;

use super::skill_drafts::{Fixture, draft, fixture};

const FOLDER: &str = "deploy-and-restart";

async fn project_uuid(fx: &Fixture) -> anyhow::Result<uuid::Uuid> {
    Ok(
        sqlx::query_scalar("SELECT project_uuid FROM projects.projects WHERE id = $1")
            .bind(fx.project_id)
            .fetch_one(fx.handle.pool())
            .await?,
    )
}

fn outcome(project: uuid::Uuid, loads: i32, corrected: i32) -> SkillLiveOutcome {
    SkillLiveOutcome {
        project,
        folder: FOLDER.to_owned(),
        first_shipped_on: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap_or_default(),
        window_start: Utc::now() - Duration::days(13),
        window_end: Utc::now(),
        loads,
        load_sessions: 4,
        loaded_turns: 9,
        judged_turns: 8,
        corrected_turns: corrected,
        base_judged_turns: 40,
        base_corrected_turns: 8,
        retired_at: None,
        retired_reason: None,
    }
}

fn target(fx: &Fixture, watermark: i64) -> DraftTarget {
    DraftTarget {
        project_id: fx.project_id,
        project_path: "/tmp/skill-drafts".to_owned(),
        folder: FOLDER.to_owned(),
        topic: FOLDER.to_owned(),
        agent_id: None,
        agent_name: None,
        lesson_watermark: watermark,
    }
}

async fn live_row(
    fx: &Fixture,
) -> anyhow::Result<(i32, i32, i32, Option<String>, Option<chrono::DateTime<Utc>>)> {
    Ok(sqlx::query_as(
        "SELECT loads, corrected_turns, base_judged_turns, retired_reason, retired_at
           FROM memory.skill_live_outcomes WHERE project_id = $1 AND folder = $2",
    )
    .bind(fx.project_id)
    .bind(FOLDER)
    .fetch_one(fx.handle.pool())
    .await?)
}

#[tokio::test]
async fn outcomes_are_stored_updated_in_place_and_a_retirement_is_never_cleared()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let project = project_uuid(&fx).await?;
    let stranger = SkillLiveOutcome {
        project: uuid::Uuid::new_v4(),
        ..outcome(project, 1, 0)
    };
    let stored = fx
        .handle
        .lessons()
        .upsert_skill_live_outcomes(&[outcome(project, 3, 1), stranger])
        .await?;
    assert_eq!(stored, 1, "a skill of an unknown project is skipped");
    assert_eq!(live_row(&fx).await?.0, 3);

    fx.handle
        .lessons()
        .upsert_skill_live_outcomes(&[outcome(project, 12, 5)])
        .await?;
    let refreshed = live_row(&fx).await?;
    assert_eq!((refreshed.0, refreshed.1, refreshed.3), (12, 5, None));

    let retired = SkillLiveOutcome {
        retired_at: Some(Utc::now()),
        retired_reason: Some(RetiredReason::CorrectedAboveBase),
        ..outcome(project, 13, 6)
    };
    fx.handle
        .lessons()
        .upsert_skill_live_outcomes(&[retired])
        .await?;
    fx.handle
        .lessons()
        .upsert_skill_live_outcomes(&[outcome(project, 14, 6)])
        .await?;
    let kept = live_row(&fx).await?;
    assert_eq!(kept.0, 14);
    assert_eq!(kept.3.as_deref(), Some("corrected_above_base"));
    assert!(kept.4.is_some());
    assert_eq!(
        fx.handle.lessons().upsert_skill_live_outcomes(&[]).await?,
        0
    );
    fx.finish().await
}

#[tokio::test]
async fn numbers_the_table_cannot_hold_are_an_error_not_a_silent_fix() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let project = project_uuid(&fx).await?;
    let impossible = SkillLiveOutcome {
        judged_turns: 99,
        ..outcome(project, 12, 5)
    };
    assert!(
        fx.handle
            .lessons()
            .upsert_skill_live_outcomes(&[impossible])
            .await
            .is_err()
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.skill_live_outcomes")
        .fetch_one(fx.handle.pool())
        .await?;
    assert_eq!(rows, 0);
    fx.finish().await
}

async fn shipped_draft(fx: &Fixture, watermark: i64) -> anyhow::Result<i64> {
    let empty = json!([]);
    let id = fx
        .handle
        .lessons()
        .insert_skill_draft(draft(&target(fx, watermark), &[1, 2], &empty))
        .await?;
    fx.handle.lessons().claim_skill_draft_for_eval().await?;
    fx.handle
        .lessons()
        .finish_skill_draft_eval(id, true, &json!({"verdict": {"accepted": true}}), None)
        .await?;
    sqlx::query(
        "UPDATE memory.skill_drafts SET created_at = now() - interval '2 days' WHERE id = $1",
    )
    .bind(id)
    .execute(fx.handle.pool())
    .await?;
    Ok(id)
}

async fn retire(fx: &Fixture, reason: RetiredReason) -> anyhow::Result<()> {
    let project = project_uuid(fx).await?;
    let retired = SkillLiveOutcome {
        retired_at: Some(Utc::now() - Duration::days(1)),
        retired_reason: Some(reason),
        ..outcome(project, 12, 5)
    };
    fx.handle
        .lessons()
        .upsert_skill_live_outcomes(&[retired])
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_retired_skill_reaches_the_next_author_with_its_numbers_and_its_shipped_text()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let wanted = target(&fx, 10);
    assert!(
        fx.handle
            .lessons()
            .retired_skill_evidence(&wanted)
            .await?
            .is_none()
    );
    let project = project_uuid(&fx).await?;
    fx.handle
        .lessons()
        .upsert_skill_live_outcomes(&[outcome(project, 12, 5)])
        .await?;
    assert!(
        fx.handle
            .lessons()
            .retired_skill_evidence(&wanted)
            .await?
            .is_none(),
        "a skill that is still shipped is not retired"
    );

    shipped_draft(&fx, 10).await?;
    retire(&fx, RetiredReason::CorrectedAboveBase).await?;
    let found = fx
        .handle
        .lessons()
        .retired_skill_evidence(&target(&fx, 20))
        .await?
        .ok_or_else(|| anyhow::anyhow!("the retirement must be found"))?;
    assert_eq!(found.numbers.reason, RetiredReason::CorrectedAboveBase);
    assert_eq!(
        (found.numbers.loads, found.numbers.corrected_turns),
        (12, 5)
    );
    assert_eq!(
        (
            found.numbers.base_judged_turns,
            found.numbers.base_corrected_turns
        ),
        (40, 8)
    );
    let shipped = found
        .shipped
        .ok_or_else(|| anyhow::anyhow!("the shipped draft must be carried"))?;
    assert_eq!(shipped.description, "Ship a migration safely.");
    assert_eq!(shipped.lesson_watermark, 10);
    fx.finish().await
}

#[tokio::test]
async fn the_retirement_stops_reaching_authors_once_a_draft_that_is_not_rejected_follows_it()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    shipped_draft(&fx, 10).await?;
    retire(&fx, RetiredReason::Unloaded).await?;
    let empty = json!([]);

    let redraft = fx
        .handle
        .lessons()
        .insert_skill_draft(draft(&target(&fx, 20), &[1, 2], &empty))
        .await?;
    assert!(
        fx.handle
            .lessons()
            .retired_skill_evidence(&target(&fx, 20))
            .await?
            .is_none(),
        "a draft written after the retirement already carries it"
    );
    fx.handle.lessons().claim_skill_draft_for_eval().await?;
    fx.handle
        .lessons()
        .finish_skill_draft_eval(
            redraft,
            false,
            &json!({"verdict": {"accepted": false}}),
            None,
        )
        .await?;
    let again = fx
        .handle
        .lessons()
        .retired_skill_evidence(&target(&fx, 20))
        .await?
        .ok_or_else(|| anyhow::anyhow!("a rejected redraft must not hide the retirement"))?;
    assert_eq!(again.numbers.reason, RetiredReason::Unloaded);
    fx.finish().await
}

#[tokio::test]
async fn the_evidence_view_joins_the_newest_evaluation_to_the_live_numbers() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let empty = json!([]);
    let id = fx
        .handle
        .lessons()
        .insert_skill_draft(draft(&target(&fx, 10), &[1, 2], &empty))
        .await?;
    fx.handle.lessons().claim_skill_draft_for_eval().await?;
    let eval = json!({
        "evaluator_version": 3,
        "calls": 3,
        "trigger": {"score": 0.85},
        "recheck": null,
        "review": {"decision": "approve", "reason": "Faithful."}
    });
    fx.handle
        .lessons()
        .finish_skill_draft_eval(id, true, &eval, None)
        .await?;
    let project = project_uuid(&fx).await?;
    fx.handle
        .lessons()
        .upsert_skill_live_outcomes(&[outcome(project, 12, 4)])
        .await?;

    let row: (String, i32, f64, String, i32, f64, f64, Option<String>) = sqlx::query_as(
        "SELECT draft_status, evaluator_version, trigger_score, review_decision, eval_calls,
                corrected_rate, base_corrected_rate, retired_reason
           FROM memory.skill_evidence_v WHERE project_id = $1 AND folder = $2",
    )
    .bind(fx.project_id)
    .bind(FOLDER)
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(row.0, "accepted");
    assert_eq!((row.1, row.3.as_str(), row.4), (3, "approve", 3));
    assert!((row.2 - 0.85).abs() < 1e-9);
    assert!((row.5 - 0.5).abs() < 1e-9);
    assert!((row.6 - 0.2).abs() < 1e-9);
    assert_eq!(row.7, None);
    fx.finish().await
}

#[tokio::test]
async fn live_queries_on_a_closed_pool_are_err() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let wanted = target(&fx, 10);
    fx.handle.pool().close().await;
    assert!(
        fx.handle
            .lessons()
            .upsert_skill_live_outcomes(&[])
            .await
            .is_err()
    );
    assert!(
        fx.handle
            .lessons()
            .retired_skill_evidence(&wanted)
            .await
            .is_err()
    );
    fx.db.cleanup().await
}
