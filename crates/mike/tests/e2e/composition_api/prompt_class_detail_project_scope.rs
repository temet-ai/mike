use crate::anthropic_fixtures;
use crate::db_helpers;
use crate::upstream;

use anthropic_fixtures::{RequestBody, cached_text_block, text_block};
use serde_json::{Value, json};

async fn fire_main_request_in_project(
    client: &reqwest::Client,
    proxy_url: &str,
    inspect_url: &str,
    pool: &sqlx::PgPool,
    prompt: &str,
    project_path: &str,
) -> anyhow::Result<i64> {
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let session_uuid = session_uuid.as_str();
    let body = RequestBody::default()
        .content(prompt)
        .max_tokens(16)
        .system(json!([
            text_block("billing-header"),
            cached_text_block("identity preamble"),
            text_block(prompt),
        ]))
        .build();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_uuid)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "proxy request failed: {}",
        resp.status()
    );
    let _ = resp.bytes().await?;

    let request_id = loop_for_request_id(pool, session_uuid, prompt).await?;

    let assign = client
        .patch(format!("{inspect_url}/api/sessions/{session_uuid}/project"))
        .json(&json!({"project_path": project_path}))
        .send()
        .await?;
    anyhow::ensure!(
        assign.status().is_success(),
        "assign session project expected 2xx, got {}: {}",
        assign.status(),
        assign.text().await.unwrap_or_default()
    );

    Ok(request_id)
}

async fn loop_for_request_id(
    pool: &sqlx::PgPool,
    session_uuid: &str,
    prompt: &str,
) -> anyhow::Result<i64> {
    for _ in 0..50 {
        if let Some(request_id) = sqlx::query_scalar::<_, i64>(
            "SELECT r.id FROM requests.requests r
               JOIN sessions.sessions s ON s.id = r.session_id
              WHERE s.session_uuid = $1::uuid
                AND r.human_prompt  = $2
              ORDER BY r.id DESC
              LIMIT 1",
        )
        .bind(session_uuid)
        .bind(prompt)
        .fetch_optional(pool)
        .await?
        {
            return Ok(request_id);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    anyhow::bail!("request for prompt {prompt:?} never appeared in requests.requests")
}

#[tokio::test]
async fn get_prompt_class_detail_main_project_path_param_narrows_to_one_project()
-> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let run_marker = uuid::Uuid::new_v4().simple().to_string();
    let project_a = format!("/tmp/prompt-class-project-a-{run_marker}");
    let project_b = format!("/tmp/prompt-class-project-b-{run_marker}");
    let prompt_a = format!("prompt-project-a-{run_marker}");
    let prompt_b = format!("prompt-project-b-{run_marker}");

    let request_a = fire_main_request_in_project(
        &client,
        &proxy_url,
        &inspect_url,
        &pool,
        &prompt_a,
        &project_a,
    )
    .await?;
    let request_b = fire_main_request_in_project(
        &client,
        &proxy_url,
        &inspect_url,
        &pool,
        &prompt_b,
        &project_b,
    )
    .await?;

    let detail_a: Value = client
        .get(format!(
            "{inspect_url}/api/prompts/classes/main?project_path={project_a}"
        ))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        detail_a["observation_count"].as_i64(),
        Some(1),
        "project_path=<project_a> must narrow the count to project A's one request: {detail_a}"
    );
    assert_eq!(
        detail_a["sample_blocks"][2]["text"].as_str(),
        Some(prompt_a.as_str()),
        "sample_blocks scoped to project A must be project A's own capture: {detail_a}"
    );
    let obs_a = detail_a["recent_observations"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("recent_observations must be an array: {detail_a}"))?;
    assert_eq!(obs_a.len(), 1, "only project A's observation: {obs_a:?}");
    assert_eq!(
        obs_a[0]["request_id"].as_str(),
        Some(request_a.to_string().as_str()),
        "the latest observation for project A must be project A's own request: {obs_a:?}"
    );

    let detail_b: Value = client
        .get(format!(
            "{inspect_url}/api/prompts/classes/main?project_path={project_b}"
        ))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(detail_b["observation_count"].as_i64(), Some(1));
    let obs_b = detail_b["recent_observations"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("recent_observations must be an array: {detail_b}"))?;
    assert_eq!(
        obs_b[0]["request_id"].as_str(),
        Some(request_b.to_string().as_str())
    );

    let detail_all: Value = client
        .get(format!("{inspect_url}/api/prompts/classes/main"))
        .send()
        .await?
        .json()
        .await?;
    assert!(
        detail_all["observation_count"].as_i64().unwrap_or(0) >= 2,
        "omitting project_path must stay unscoped, matching pre-existing behaviour: {detail_all}"
    );
    let all_ids: Vec<&str> = detail_all["recent_observations"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("recent_observations must be an array: {detail_all}"))?
        .iter()
        .filter_map(|o| o["request_id"].as_str())
        .collect();
    assert!(all_ids.contains(&request_a.to_string().as_str()));
    assert!(all_ids.contains(&request_b.to_string().as_str()));

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_prompt_classes_project_path_param_narrows_catalogue_rows() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let run_marker = uuid::Uuid::new_v4().simple().to_string();
    let project_a = format!("/tmp/prompt-class-catalogue-a-{run_marker}");
    let project_b = format!("/tmp/prompt-class-catalogue-b-{run_marker}");
    let prompt_a = format!("catalogue-project-a-{run_marker}");
    let prompt_b = format!("catalogue-project-b-{run_marker}");

    fire_main_request_in_project(
        &client,
        &proxy_url,
        &inspect_url,
        &pool,
        &prompt_a,
        &project_a,
    )
    .await?;
    fire_main_request_in_project(
        &client,
        &proxy_url,
        &inspect_url,
        &pool,
        &prompt_b,
        &project_b,
    )
    .await?;

    let classes_a: Value = client
        .get(format!(
            "{inspect_url}/api/prompts/classes?project_path={project_a}"
        ))
        .send()
        .await?
        .json()
        .await?;
    let rows_a = classes_a
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected an array, got {classes_a}"))?;
    let main_row_a = rows_a
        .iter()
        .find(|r| r["lane"].as_str() == Some("main"))
        .ok_or_else(|| {
            anyhow::anyhow!("no lane=main row scoped to project A: {rows_a:?}, marker {run_marker}")
        })?;
    assert_eq!(
        main_row_a["observation_count"].as_i64(),
        Some(1),
        "project_path=<project_a> must narrow the catalogue's main row to project A's one \
         observation: {main_row_a}"
    );

    let classes_all: Value = client
        .get(format!("{inspect_url}/api/prompts/classes"))
        .send()
        .await?
        .json()
        .await?;
    let rows_all = classes_all
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected an array, got {classes_all}"))?;
    let main_row_all = rows_all
        .iter()
        .find(|r| r["lane"].as_str() == Some("main"))
        .ok_or_else(|| anyhow::anyhow!("no lane=main row in unscoped catalogue: {rows_all:?}"))?;
    assert!(
        main_row_all["observation_count"].as_i64().unwrap_or(0) >= 2,
        "omitting project_path must stay unscoped and count both projects' requests: \
         {main_row_all}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
