use mike::lesson_proposer::skill_author::build_input;
use mike::lesson_proposer::skill_eval::EVALUATOR_VERSION;
use mike::store::persistence::lessons::skill_drafts::DraftTarget;
use serde_json::{Value, json};

use super::skill_drafts::{Fixture, draft, fixture};

fn target_for(fx: &Fixture, folder: &str, watermark: i64) -> DraftTarget {
    DraftTarget {
        project_id: fx.project_id,
        project_path: "/tmp/skill-drafts".to_owned(),
        folder: folder.to_owned(),
        topic: folder.to_owned(),
        agent_id: None,
        agent_name: None,
        lesson_watermark: watermark,
    }
}

pub(super) async fn insert(
    fx: &Fixture,
    folder: &str,
    watermark: i64,
    description: &str,
) -> anyhow::Result<i64> {
    let target = target_for(fx, folder, watermark);
    let empty = json!([]);
    let mut new = draft(&target, &[1, 2], &empty);
    new.description = description;
    Ok(fx.handle.insert_skill_draft(new).await?)
}

async fn status_of(fx: &Fixture, id: i64) -> anyhow::Result<String> {
    Ok(
        sqlx::query_scalar("SELECT status FROM memory.skill_drafts WHERE id = $1")
            .bind(id)
            .fetch_one(fx.handle.pool())
            .await?,
    )
}

async fn age_eval_failures(fx: &Fixture, hours: i32) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.skill_draft_eval_failures
            SET last_failed_at = now() - make_interval(hours => $1)",
    )
    .bind(hours)
    .execute(fx.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn claim_takes_the_draft_being_evaluated_before_the_oldest_waiting_one() -> anyhow::Result<()>
{
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let first = insert(&fx, "first", 10, "one").await?;
    let second = insert(&fx, "second", 20, "two").await?;

    let claimed = fx
        .handle
        .claim_skill_draft_for_eval()
        .await?
        .ok_or_else(|| anyhow::anyhow!("a waiting draft must be claimed"))?;
    assert_eq!(claimed.id, first);
    assert_eq!(status_of(&fx, first).await?, "evaluating");
    assert!(claimed.eval_progress.is_none());

    let again = fx
        .handle
        .claim_skill_draft_for_eval()
        .await?
        .ok_or_else(|| anyhow::anyhow!("the draft being evaluated must be claimed again"))?;
    assert_eq!(again.id, first, "work in progress comes before a new draft");
    assert_eq!(status_of(&fx, second).await?, "draft");
    fx.finish().await
}

#[tokio::test]
async fn progress_is_saved_returned_on_the_next_claim_and_cleared_by_the_result()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = insert(&fx, "first", 10, "one").await?;
    fx.handle.claim_skill_draft_for_eval().await?;

    let progress = json!({"trigger": [], "prompts": [], "models": ["m"]});
    assert!(fx.handle.save_skill_draft_progress(id, &progress).await?);
    let claimed = fx
        .handle
        .claim_skill_draft_for_eval()
        .await?
        .ok_or_else(|| anyhow::anyhow!("claim"))?;
    assert_eq!(claimed.eval_progress, Some(progress.clone()));

    let eval = json!({"verdict": {"accepted": true}});
    assert!(
        fx.handle
            .finish_skill_draft_eval(id, true, &eval, None)
            .await?
    );
    let (status, stored, progress_left, evaluated): (String, Option<Value>, Option<Value>, bool) =
        sqlx::query_as(
            "SELECT status, eval, eval_progress, evaluated_at IS NOT NULL
               FROM memory.skill_drafts WHERE id = $1",
        )
        .bind(id)
        .fetch_one(fx.handle.pool())
        .await?;
    assert_eq!(status, "accepted");
    assert_eq!(stored, Some(eval.clone()));
    assert!(progress_left.is_none() && evaluated);

    assert!(
        !fx.handle
            .finish_skill_draft_eval(id, false, &eval, None)
            .await?,
        "a decided draft must not be decided twice"
    );
    assert!(!fx.handle.save_skill_draft_progress(id, &progress).await?);
    assert!(fx.handle.claim_skill_draft_for_eval().await?.is_none());
    fx.finish().await
}

#[tokio::test]
async fn a_failing_draft_waits_then_is_rejected_on_the_third_failure() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let id = insert(&fx, "first", 10, "one").await?;
    fx.handle.claim_skill_draft_for_eval().await?;
    fx.handle
        .save_skill_draft_progress(id, &json!({"trigger": []}))
        .await?;

    let first = fx
        .handle
        .fail_skill_draft_eval(id, "model: 502", 3, EVALUATOR_VERSION)
        .await?
        .ok_or_else(|| anyhow::anyhow!("failure recorded"))?;
    assert_eq!((first.attempts, first.rejected), (1, false));
    assert!(
        fx.handle.claim_skill_draft_for_eval().await?.is_none(),
        "a draft that just failed must wait"
    );

    age_eval_failures(&fx, 3).await?;
    let retried = fx
        .handle
        .claim_skill_draft_for_eval()
        .await?
        .ok_or_else(|| anyhow::anyhow!("retry after the wait"))?;
    assert_eq!(retried.eval_progress, Some(json!({"trigger": []})));

    fx.handle
        .fail_skill_draft_eval(id, "model: 502", 3, EVALUATOR_VERSION)
        .await?;
    let third = fx
        .handle
        .fail_skill_draft_eval(id, "model: 503", 3, EVALUATOR_VERSION)
        .await?
        .ok_or_else(|| anyhow::anyhow!("third failure recorded"))?;
    assert_eq!((third.attempts, third.rejected), (3, true));
    assert_eq!(status_of(&fx, id).await?, "rejected");
    let (eval, progress): (Option<Value>, Option<Value>) =
        sqlx::query_as("SELECT eval, eval_progress FROM memory.skill_drafts WHERE id = $1")
            .bind(id)
            .fetch_one(fx.handle.pool())
            .await?;
    assert_eq!(
        eval,
        Some(json!({
            "evaluator_version": EVALUATOR_VERSION,
            "error": "model: 503",
            "attempts": 3
        }))
    );
    assert!(progress.is_none());
    age_eval_failures(&fx, 1000).await?;
    assert!(fx.handle.claim_skill_draft_for_eval().await?.is_none());
    fx.finish().await
}

#[tokio::test]
async fn siblings_are_the_other_live_drafts_of_the_project_accepted_first() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let own = insert(&fx, "own", 10, "own description").await?;
    insert(&fx, "waiting", 10, "waiting description").await?;
    let accepted = insert(&fx, "shipped", 10, "shipped description").await?;
    let dropped = insert(&fx, "dropped", 10, "dropped description").await?;
    insert(&fx, "shipped", 5, "older shipped description").await?;
    sqlx::query("UPDATE memory.skill_drafts SET status = 'accepted' WHERE id = $1")
        .bind(accepted)
        .execute(fx.handle.pool())
        .await?;
    sqlx::query("UPDATE memory.skill_drafts SET status = 'rejected' WHERE id = $1")
        .bind(dropped)
        .execute(fx.handle.pool())
        .await?;

    let siblings = fx
        .handle
        .skill_draft_siblings(fx.project_id, "own", 5)
        .await?;
    let names: Vec<&str> = siblings.iter().map(|s| s.folder.as_str()).collect();
    assert_eq!(names, vec!["shipped", "waiting"]);
    assert_eq!(siblings[0].description, "shipped description");
    assert!(!names.contains(&"own") && !names.contains(&"dropped"));
    assert_eq!(
        fx.handle
            .skill_draft_siblings(fx.project_id, "own", 1)
            .await?
            .len(),
        1
    );
    assert!(status_of(&fx, own).await? == "draft");
    fx.finish().await
}

#[tokio::test]
async fn the_replaced_draft_is_the_newest_older_accepted_one_in_the_drafts_own_folders()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let old = insert(&fx, "topic-agent", 5, "old accepted").await?;
    let newer = insert(&fx, "topic-agent", 10, "newer accepted").await?;
    let bare = insert(&fx, "topic", 20, "bare accepted").await?;
    let waiting = insert(&fx, "topic-agent", 30, "waiting").await?;
    let other = insert(&fx, "other", 40, "other accepted").await?;
    for id in [old, newer, bare, other] {
        sqlx::query("UPDATE memory.skill_drafts SET status = 'accepted' WHERE id = $1")
            .bind(id)
            .execute(fx.handle.pool())
            .await?;
    }
    let own = vec!["topic-agent".to_owned(), "topic".to_owned()];
    let replaced = fx
        .handle
        .skill_draft_replaced(fx.project_id, &own, waiting)
        .await?;
    assert_eq!(
        replaced.map(|r| (r.folder, r.description)),
        Some(("topic".to_owned(), "bare accepted".to_owned()))
    );
    let exact = vec!["topic-agent".to_owned()];
    let replaced = fx
        .handle
        .skill_draft_replaced(fx.project_id, &exact, waiting)
        .await?;
    assert_eq!(
        replaced.map(|r| r.description),
        Some("newer accepted".to_owned())
    );
    assert!(
        fx.handle
            .skill_draft_replaced(fx.project_id, &exact, old)
            .await?
            .is_none()
    );
    sqlx::query("UPDATE memory.skill_drafts SET review = 'rejected' WHERE id = $1")
        .bind(newer)
        .execute(fx.handle.pool())
        .await?;
    let replaced = fx
        .handle
        .skill_draft_replaced(fx.project_id, &exact, waiting)
        .await?;
    assert_eq!(
        replaced.map(|r| r.description),
        Some("old accepted".to_owned())
    );
    fx.finish().await
}

#[tokio::test]
async fn a_rejected_drafts_evaluation_reaches_the_next_author_input_up_to_three_drafts()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let a = fx.daily_lesson("Run migrate-check first.").await?;
    let b = fx
        .daily_lesson("Commit migrations with their code.")
        .await?;
    let eval = json!({
        "verdict": {"accepted": false, "reasons": ["the draft's trigger score 0.55 is below 0.80"]},
        "review": {"decision": "approve", "reason": "Faithful to the lessons."},
        "findings": {
            "missed_triggers": [{"query": "renumber my sqlx migration", "loaded": []}],
            "wrong_triggers": []
        }
    });
    let mut last_input = String::new();
    for revision in 1..=3 {
        let target = fx
            .handle
            .next_skill_draft_target()
            .await?
            .ok_or_else(|| anyhow::anyhow!("revision {revision} must be offered"))?;
        assert_eq!(target.lesson_watermark, b);
        let previous = fx.handle.previous_rejected_skill_draft(&target).await?;
        let lessons = fx.handle.skill_draft_lessons(&target).await?;
        last_input = build_input(&target, &lessons, previous.as_ref(), None);
        assert_eq!(
            previous.is_some(),
            revision > 1,
            "only a redraft carries a previous draft"
        );
        let empty = json!([]);
        let id = fx
            .handle
            .insert_skill_draft(draft(&target, &[a, b], &empty))
            .await?;
        assert!(
            fx.handle.next_skill_draft_target().await?.is_none(),
            "a draft that is not decided yet holds the topic"
        );
        fx.handle.claim_skill_draft_for_eval().await?;
        fx.handle
            .finish_skill_draft_eval(id, false, &eval, None)
            .await?;
    }
    assert!(last_input.contains("## PREVIOUS DRAFT (REJECTED)"));
    assert!(last_input.contains("\"renumber my sqlx migration\""));
    assert!(last_input.contains("Rejected because the draft's trigger score 0.55 is below 0.80."));
    assert!(last_input.contains("The reviewer's decision was approve: Faithful to the lessons."));
    assert!(
        fx.handle.next_skill_draft_target().await?.is_none(),
        "three rejected drafts at one watermark must stop the topic"
    );

    fx.daily_lesson("Number a migration above the applied one.")
        .await?;
    let reopened = fx
        .handle
        .next_skill_draft_target()
        .await?
        .ok_or_else(|| anyhow::anyhow!("a new lesson must reopen the topic"))?;
    let previous = fx.handle.previous_rejected_skill_draft(&reopened).await?;
    assert_eq!(previous.map(|p| p.revision), Some(3));
    fx.finish().await
}

#[tokio::test]
async fn an_accepted_or_waiting_draft_is_not_a_previous_draft() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let target = target_for(&fx, "first", 10);
    assert!(
        fx.handle
            .previous_rejected_skill_draft(&target)
            .await?
            .is_none()
    );
    let id = insert(&fx, "first", 10, "one").await?;
    assert!(
        fx.handle
            .previous_rejected_skill_draft(&target)
            .await?
            .is_none()
    );
    fx.handle.claim_skill_draft_for_eval().await?;
    fx.handle
        .finish_skill_draft_eval(id, true, &json!({"verdict": {"accepted": true}}), None)
        .await?;
    assert!(
        fx.handle
            .previous_rejected_skill_draft(&target)
            .await?
            .is_none()
    );
    fx.finish().await
}

#[tokio::test]
async fn the_project_path_of_a_draft_is_looked_up_by_project_id() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    assert_eq!(
        fx.handle
            .skill_draft_project_path(fx.project_id)
            .await?
            .as_deref(),
        Some("/tmp/skill-drafts")
    );
    assert_eq!(fx.handle.skill_draft_project_path(-1).await?, None);
    fx.db.cleanup().await
}

#[tokio::test]
async fn eval_queries_on_a_closed_pool_are_err() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let target = target_for(&fx, "first", 10);
    fx.handle.pool().close().await;
    assert!(fx.handle.skill_draft_project_path(1).await.is_err());
    assert!(fx.handle.claim_skill_draft_for_eval().await.is_err());
    assert!(
        fx.handle
            .skill_draft_siblings(fx.project_id, "x", 5)
            .await
            .is_err()
    );
    assert!(
        fx.handle
            .save_skill_draft_progress(1, &json!({}))
            .await
            .is_err()
    );
    assert!(
        fx.handle
            .finish_skill_draft_eval(1, true, &json!({}), None)
            .await
            .is_err()
    );
    assert!(
        fx.handle
            .fail_skill_draft_eval(1, "x", 3, EVALUATOR_VERSION)
            .await
            .is_err()
    );
    assert!(
        fx.handle
            .previous_rejected_skill_draft(&target)
            .await
            .is_err()
    );
    fx.db.cleanup().await
}
