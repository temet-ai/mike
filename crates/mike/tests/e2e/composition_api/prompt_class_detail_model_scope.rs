use crate::anthropic_fixtures;
use crate::db_helpers;
use crate::upstream;

use anthropic_fixtures::{RequestBody, cached_text_block, text_block};
use serde_json::{Value, json};

async fn fire_main_request_with_model(
    client: &reqwest::Client,
    proxy_url: &str,
    pool: &sqlx::PgPool,
    session_uuid: &str,
    prompt: &str,
    model: &str,
) -> anyhow::Result<i64> {
    let body = RequestBody::default()
        .model(model)
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
async fn get_prompt_class_detail_main_model_param_narrows_to_one_model() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let run_marker = uuid::Uuid::new_v4().simple().to_string();
    let model_a = format!("claude-model-a-{run_marker}");
    let model_b = format!("claude-model-b-{run_marker}");
    let prompt_a = format!("prompt-model-a-{run_marker}");
    let prompt_b = format!("prompt-model-b-{run_marker}");

    let request_a = fire_main_request_with_model(
        &client,
        &proxy_url,
        &pool,
        &uuid::Uuid::new_v4().to_string(),
        &prompt_a,
        &model_a,
    )
    .await?;
    let request_b = fire_main_request_with_model(
        &client,
        &proxy_url,
        &pool,
        &uuid::Uuid::new_v4().to_string(),
        &prompt_b,
        &model_b,
    )
    .await?;

    let detail_a: Value = client
        .get(format!(
            "{inspect_url}/api/prompts/classes/main?model={model_a}"
        ))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        detail_a["observation_count"].as_i64(),
        Some(1),
        "model=<model_a> must narrow the count to model A's one request: {detail_a}"
    );
    assert_eq!(
        detail_a["sample_blocks"][2]["text"].as_str(),
        Some(prompt_a.as_str()),
        "sample_blocks scoped to model A must be model A's own capture: {detail_a}"
    );
    let obs_a = detail_a["recent_observations"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("recent_observations must be an array: {detail_a}"))?;
    assert_eq!(obs_a.len(), 1, "only model A's observation: {obs_a:?}");
    assert_eq!(
        obs_a[0]["request_id"].as_str(),
        Some(request_a.to_string().as_str())
    );
    assert_eq!(
        obs_a[0]["model"].as_str(),
        Some(model_a.as_str()),
        "every observation must carry its own model: {obs_a:?}"
    );

    let detail_b: Value = client
        .get(format!(
            "{inspect_url}/api/prompts/classes/main?model={model_b}"
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
    assert_eq!(obs_b[0]["model"].as_str(), Some(model_b.as_str()));

    let detail_all: Value = client
        .get(format!("{inspect_url}/api/prompts/classes/main"))
        .send()
        .await?
        .json()
        .await?;
    assert!(
        detail_all["observation_count"].as_i64().unwrap_or(0) >= 2,
        "omitting model must stay unscoped: {detail_all}"
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
