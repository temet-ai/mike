use crate::db_helpers;

use anyhow::Context as _;
use serde_json::Value;

use super::support::{
    ActionFixture, bootstrap_project_session, insert_action, insert_lesson, start_consolidation_run,
};

#[tokio::test]
async fn get_lesson_history_returns_full_chain_or_404() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, _test_db) = proxy.into_parts();

    let project_path = "test-lesson-history";
    let session_id = "66666666-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;
    let now = chrono::Utc::now();
    let run_id = start_consolidation_run(&pool, "session", "manual", "completed", now).await?;
    let (lesson_id, lesson_uuid) =
        insert_lesson(&pool, project_id, "the tracked lesson", "long_term").await?;

    insert_action(
        &pool,
        ActionFixture {
            run_id,
            lesson_id,
            lesson_uuid,
            project_id: Some(project_id),
            session_uuid: Some(session_id),
            files_touched: None,
            lesson_text: "the tracked lesson",
            action_kind: "promoted_to_long_term",
            outcome: "applied",
            error: None,
            distance: None,
            threshold: None,
            counterpart_lesson_id: None,
            counterpart_uuid: None,
            counterpart_text: None,
            runners_up: serde_json::json!([]),
            created_at: now,
        },
    )
    .await?;

    let empty_obj = serde_json::json!({});
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests
            (session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
             status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
             created_at)
         SELECT s.id, $2, m.id, '/v1/messages', false, 1, false,
                st.id, $4, '', 'history fixture prompt', ttype.id, torg.id,
                $3
           FROM sessions.sessions s,
                requests.methods m,
                requests.turn_origins torg,
                requests.turn_types ttype,
                requests.statuses st
          WHERE s.session_uuid = $1::text::uuid
            AND m.code = 'POST' AND torg.code = 'primary' AND ttype.code = 'user_prompt'
            AND st.code = 'complete'
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .bind(&empty_obj)
    .fetch_one(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO memory.lesson_injections (lesson_id, request_id, session_id, lane_id, persona)
         SELECT $1, $2, s.id, lane.id, NULL
           FROM sessions.sessions s, memory.lesson_injection_lanes lane
          WHERE s.session_uuid = $3::text::uuid AND lane.code = 'primary'",
    )
    .bind(lesson_id)
    .bind(request_id)
    .bind(session_id)
    .execute(&pool)
    .await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{inspect_url}/api/lessons/{lesson_id}/history"))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await?;
    assert_eq!(body["lesson_id"], lesson_id);
    assert_eq!(body["lesson_text"], "the tracked lesson");
    assert_eq!(body["stage"], "long_term");
    assert_eq!(
        body["actions"].as_array().context("actions array")?.len(),
        1
    );
    assert_eq!(body["actions"][0]["action_kind"], "promoted_to_long_term");
    let injections = body["injections"].as_array().context("injections array")?;
    assert_eq!(injections.len(), 1);
    assert_eq!(injections[0]["lane"], "primary");
    assert_eq!(injections[0]["request_id"], request_id);

    let resp = client
        .get(format!(
            "{inspect_url}/api/lessons/{}/history",
            lesson_id + 999_999
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

    pool.close().await;
    Ok(())
}
