use mike::lesson_proposer::skill_eval::EVALUATOR_VERSION;
use mike::store::persistence::lessons::skill_draft_eval::ListingWindow;
use mike::store::persistence::lessons::skill_drafts::DraftTarget;
use mike::store::persistence::lessons_repo::LessonsPersist as _;
use mike::store::test_support;
use serde_json::{Value, json};

use super::skill_drafts::{Fixture, draft, fixture};

const MARKER: &str = "available for use with the Skill tool:";

pub(super) const WIDE: ListingWindow = ListingWindow {
    candidates: 24,
    sessions: 5,
    min_bytes: 1,
    max_bytes: 10_000_000,
};

pub(super) fn listing_body(names: &[&str], padding: usize) -> String {
    let mut text = format!("The following skills are {MARKER}\n\n");
    for name in names {
        text.push_str(&format!("- {name}: does {name} things\n"));
    }
    json!({
        "system": [{"type": "text", "text": text}],
        "messages": [{"role": "user", "content": "x".repeat(padding)}]
    })
    .to_string()
}

pub(super) async fn session(fx: &Fixture, project_id: i64) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES (gen_random_uuid(), $1, now(), now()) RETURNING id",
    )
    .bind(project_id)
    .fetch_one(fx.handle.pool())
    .await?)
}

pub(super) async fn request(
    fx: &Fixture,
    project_id: i64,
    session_id: i64,
    body: &str,
    minutes_ago: i32,
) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count,
            is_tool_loop, status_id, params, last_user_message, human_prompt,
            turn_type_id, turn_origin_id, request_body, request_body_size, created_at)
         SELECT $1, $2, rm.id, '/v1/messages', false, 1, false, rs.id, '{}'::jsonb,
                'x', 'x', tt.id, ro.id, convert_to($3, 'UTF8'),
                octet_length(convert_to($3, 'UTF8')),
                now() - make_interval(mins => $4)
           FROM requests.methods rm, requests.statuses rs, requests.turn_types tt,
                requests.turn_origins ro
          WHERE rm.code = 'POST' AND rs.code = 'complete' AND tt.code = 'user_prompt'
            AND ro.code = 'primary'
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(body)
    .bind(minutes_ago)
    .fetch_one(fx.handle.pool())
    .await?)
}

#[tokio::test]
async fn listing_bodies_are_the_newest_carrying_request_of_each_session_of_the_project()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let (first, second) = (
        session(&fx, fx.project_id).await?,
        session(&fx, fx.project_id).await?,
    );
    let old = request(&fx, fx.project_id, first, &listing_body(&["a"], 10), 50).await?;
    let new = request(
        &fx,
        fx.project_id,
        first,
        &listing_body(&["a", "b"], 10),
        40,
    )
    .await?;
    let plain = request(&fx, fx.project_id, first, "{\"messages\": []}", 30).await?;
    let other_session = request(&fx, fx.project_id, second, &listing_body(&["c"], 10), 20).await?;
    let huge = request(&fx, fx.project_id, second, &listing_body(&["d"], 5_000), 10).await?;
    let stranger = test_support::seed_project(fx.handle.pool(), "/tmp/someone-else").await?;
    let foreign = session(&fx, stranger).await?;
    request(&fx, stranger, foreign, &listing_body(&["z"], 10), 5).await?;

    let narrow = ListingWindow {
        max_bytes: 1_000,
        ..WIDE
    };
    let found = fx
        .handle
        .lessons()
        .recent_skill_listing_bodies(fx.project_id, MARKER.as_bytes(), narrow)
        .await?;
    let ids: Vec<i64> = found.iter().map(|b| b.request_id).collect();
    assert_eq!(
        ids,
        vec![other_session, new],
        "huge={huge} plain={plain} old={old}"
    );

    let everything = fx
        .handle
        .lessons()
        .recent_skill_listing_bodies(fx.project_id, MARKER.as_bytes(), WIDE)
        .await?;
    assert_eq!(everything.first().map(|b| b.request_id), Some(huge));
    assert_eq!(everything.len(), 2);

    let limited = ListingWindow {
        sessions: 1,
        ..WIDE
    };
    assert_eq!(
        fx.handle
            .lessons()
            .recent_skill_listing_bodies(fx.project_id, MARKER.as_bytes(), limited)
            .await?
            .len(),
        1
    );
    fx.finish().await
}

#[tokio::test]
async fn a_project_without_captured_requests_has_no_listing_bodies() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let found = fx
        .handle
        .lessons()
        .recent_skill_listing_bodies(fx.project_id, MARKER.as_bytes(), WIDE)
        .await?;
    assert!(found.is_empty());
    fx.finish().await
}

async fn rejected_draft(fx: &Fixture, watermark: i64, eval: Option<Value>) -> anyhow::Result<i64> {
    let target = DraftTarget {
        project_id: fx.project_id,
        project_path: "/tmp/skill-drafts".to_owned(),
        folder: format!("topic-{watermark}"),
        topic: format!("topic-{watermark}"),
        agent_id: None,
        agent_name: None,
        lesson_watermark: watermark,
    };
    let empty = json!([]);
    let id = fx
        .handle
        .lessons()
        .insert_skill_draft(draft(&target, &[1, 2], &empty))
        .await?;
    sqlx::query(
        "UPDATE memory.skill_drafts
            SET status = 'rejected', eval = $2, evaluated_at = now(),
                eval_progress = '{\"x\": 1}'::jsonb
          WHERE id = $1",
    )
    .bind(id)
    .bind(eval)
    .execute(fx.handle.pool())
    .await?;
    Ok(id)
}

async fn state(fx: &Fixture, id: i64) -> anyhow::Result<(String, Option<Value>, bool, bool)> {
    Ok(sqlx::query_as(
        "SELECT status, eval, evaluated_at IS NOT NULL, eval_progress IS NOT NULL
           FROM memory.skill_drafts WHERE id = $1",
    )
    .bind(id)
    .fetch_one(fx.handle.pool())
    .await?)
}

#[tokio::test]
async fn drafts_rejected_by_an_older_evaluator_go_back_to_the_queue_once() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let unversioned = rejected_draft(&fx, 1, Some(json!({"version": 1, "verdict": {}}))).await?;
    let failed_old = rejected_draft(&fx, 2, Some(json!({"error": "model", "attempts": 3}))).await?;
    let current = rejected_draft(
        &fx,
        3,
        Some(json!({"evaluator_version": EVALUATOR_VERSION, "verdict": {}})),
    )
    .await?;
    let accepted = rejected_draft(&fx, 4, Some(json!({"version": 1}))).await?;
    sqlx::query("UPDATE memory.skill_drafts SET status = 'accepted' WHERE id = $1")
        .bind(accepted)
        .execute(fx.handle.pool())
        .await?;
    sqlx::query(
        "INSERT INTO memory.skill_draft_eval_failures (draft_id, last_error) VALUES ($1, 'x')",
    )
    .bind(failed_old)
    .execute(fx.handle.pool())
    .await?;

    let requeued = fx
        .handle
        .lessons()
        .requeue_skill_drafts_before_evaluator(EVALUATOR_VERSION)
        .await?;
    assert_eq!(requeued, 2);

    for id in [unversioned, failed_old] {
        assert_eq!(
            state(&fx, id).await?,
            ("draft".to_owned(), None, false, false)
        );
    }
    assert_eq!(state(&fx, current).await?.0, "rejected");
    assert!(state(&fx, current).await?.1.is_some());
    assert_eq!(state(&fx, accepted).await?.0, "accepted");
    let failures: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.skill_draft_eval_failures")
        .fetch_one(fx.handle.pool())
        .await?;
    assert_eq!(failures, 0);

    assert_eq!(
        fx.handle
            .lessons()
            .requeue_skill_drafts_before_evaluator(EVALUATOR_VERSION)
            .await?,
        0,
        "a second pass finds nothing left to requeue"
    );
    fx.finish().await
}
