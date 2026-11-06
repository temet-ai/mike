use crate::anthropic_fixtures;
use crate::db_helpers;
use crate::upstream;

use anthropic_fixtures::{RequestBody, cached_text_block, text_block};
use serde_json::{Value, json};

use super::support::fire_request_and_get_session_and_request_ids;

struct FireSpec<'a> {
    session_uuid: &'a str,
    prompt: &'a str,
    model: &'a str,
    cc_version: &'a str,
}

async fn fire_main_request_with_release(
    client: &reqwest::Client,
    proxy_url: &str,
    pool: &sqlx::PgPool,
    spec: FireSpec<'_>,
) -> anyhow::Result<i64> {
    let billing_header = format!(
        "x-anthropic-billing-header: cc_version={}; cc_entrypoint=cli; cch=deadbeef;",
        spec.cc_version
    );
    let body = RequestBody::default()
        .model(spec.model)
        .content(spec.prompt)
        .max_tokens(16)
        .system(json!([
            text_block(billing_header.as_str()),
            cached_text_block("identity preamble"),
            text_block(spec.prompt),
        ]))
        .build();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", spec.session_uuid)
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
        .bind(spec.session_uuid)
        .bind(spec.prompt)
        .fetch_optional(pool)
        .await?
        {
            return Ok(request_id);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    anyhow::bail!(
        "request for prompt {:?} never appeared in requests.requests",
        spec.prompt
    )
}

struct ReleaseGroupIds {
    model_a: String,
    model_b: String,
    old_a: i64,
    old_b: i64,
    new_a1: i64,
    new_a2: i64,
    new_b: i64,
}

async fn seed_release_group_observations(
    client: &reqwest::Client,
    proxy_url: &str,
    pool: &sqlx::PgPool,
    run_marker: &str,
) -> anyhow::Result<ReleaseGroupIds> {
    let model_a = format!("claude-model-a-{run_marker}");
    let model_b = format!("claude-model-b-{run_marker}");

    let old_a = fire_main_request_with_release(
        client,
        proxy_url,
        pool,
        FireSpec {
            session_uuid: &uuid::Uuid::new_v4().to_string(),
            prompt: &format!("versions-old-a-{run_marker}"),
            model: &model_a,
            cc_version: "2.1.257.abc",
        },
    )
    .await?;
    let old_b = fire_main_request_with_release(
        client,
        proxy_url,
        pool,
        FireSpec {
            session_uuid: &uuid::Uuid::new_v4().to_string(),
            prompt: &format!("versions-old-b-{run_marker}"),
            model: &model_b,
            cc_version: "2.1.257.abc",
        },
    )
    .await?;
    let new_a1 = fire_main_request_with_release(
        client,
        proxy_url,
        pool,
        FireSpec {
            session_uuid: &uuid::Uuid::new_v4().to_string(),
            prompt: &format!("versions-new-a1-{run_marker}"),
            model: &model_a,
            cc_version: "2.1.263.des",
        },
    )
    .await?;
    let new_a2 = fire_main_request_with_release(
        client,
        proxy_url,
        pool,
        FireSpec {
            session_uuid: &uuid::Uuid::new_v4().to_string(),
            prompt: &format!("versions-new-a2-{run_marker}"),
            model: &model_a,
            cc_version: "2.1.263.beta",
        },
    )
    .await?;
    let new_b = fire_main_request_with_release(
        client,
        proxy_url,
        pool,
        FireSpec {
            session_uuid: &uuid::Uuid::new_v4().to_string(),
            prompt: &format!("versions-new-b-{run_marker}"),
            model: &model_b,
            cc_version: "2.1.263.des",
        },
    )
    .await?;

    Ok(ReleaseGroupIds {
        model_a,
        model_b,
        old_a,
        old_b,
        new_a1,
        new_a2,
        new_b,
    })
}

fn assert_release_versions(body: &Value, ids: &ReleaseGroupIds) -> anyhow::Result<()> {
    anyhow::ensure!(body["lane"].as_str() == Some("main"), "lane: {body}");
    anyhow::ensure!(
        body["subagent_persona"].is_null(),
        "subagent_persona: {body}"
    );

    let versions = body["versions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("versions must be an array: {body}"))?;
    let index_of = |v: &str| {
        versions
            .iter()
            .position(|e| e["version"].as_str() == Some(v))
    };
    let current_idx =
        index_of("2.1.263").ok_or_else(|| anyhow::anyhow!("no 2.1.263 release: {versions:?}"))?;
    let older_idx =
        index_of("2.1.257").ok_or_else(|| anyhow::anyhow!("no 2.1.257 release: {versions:?}"))?;
    anyhow::ensure!(
        current_idx < older_idx,
        "2.1.263 must sort before 2.1.257 (last_seen desc): {versions:?}"
    );

    let current = &versions[current_idx];
    let older = &versions[older_idx];
    anyhow::ensure!(
        current["builds"].as_i64() == Some(2),
        "2.1.263 builds: {current}"
    );
    anyhow::ensure!(
        current["observation_count"].as_i64() == Some(3),
        "2.1.263 observation_count: {current}"
    );
    anyhow::ensure!(
        older["builds"].as_i64() == Some(1),
        "2.1.257 builds: {older}"
    );
    anyhow::ensure!(
        older["observation_count"].as_i64() == Some(2),
        "2.1.257 observation_count: {older}"
    );

    assert_model_row(
        &current["models"],
        &ids.model_a,
        2,
        ids.new_a2,
        Some(ids.new_a1),
    )?;
    assert_model_row(&current["models"], &ids.model_b, 1, ids.new_b, None)?;
    anyhow::ensure!(
        current["models"][0]["model"].as_str() == Some(ids.model_a.as_str()),
        "models must be count-descending, model A (2 obs) before model B (1 obs): {current}"
    );
    assert_model_row(&older["models"], &ids.model_a, 1, ids.old_a, None)?;
    assert_model_row(&older["models"], &ids.model_b, 1, ids.old_b, None)?;
    Ok(())
}

fn assert_model_row(
    models: &Value,
    model: &str,
    observation_count: i64,
    latest_request_id: i64,
    not_latest: Option<i64>,
) -> anyhow::Result<()> {
    let rows = models
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("models must be an array: {models}"))?;
    let row = rows
        .iter()
        .find(|m| m["model"].as_str() == Some(model))
        .ok_or_else(|| anyhow::anyhow!("model {model} missing from {rows:?}"))?;
    anyhow::ensure!(
        row["observation_count"].as_i64() == Some(observation_count),
        "model {model} observation_count: {row}"
    );
    anyhow::ensure!(
        row["latest_request_id"].as_str() == Some(latest_request_id.to_string().as_str()),
        "model {model} latest_request_id: {row}"
    );
    if let Some(older) = not_latest {
        anyhow::ensure!(
            row["latest_request_id"].as_str() != Some(older.to_string().as_str()),
            "model {model} latest_request_id must not be the older observation {older}: {row}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn get_prompt_class_versions_main_groups_by_release_then_model() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let run_marker = uuid::Uuid::new_v4().simple().to_string();
    let ids = seed_release_group_observations(&client, &proxy_url, &pool, &run_marker).await?;

    let resp = client
        .get(format!("{inspect_url}/api/prompts/classes/main/versions"))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    assert_release_versions(&body, &ids)?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_prompt_class_versions_subagent_unresolved_and_unknown_persona() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let run_marker = uuid::Uuid::new_v4().simple().to_string();
    let prompt = format!("versions-subagent-unresolved-{run_marker}");
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let (session_id, request_id) = fire_request_and_get_session_and_request_ids(
        &client,
        &proxy_url,
        &pool,
        &session_uuid,
        &prompt,
    )
    .await?;

    let blocks = json!([
        {"type": "text", "text": "x-anthropic-billing-header: cc_version=9.9.9.test; cc_entrypoint=cli;"},
        {"type": "text", "text": "identity preamble"},
        {"type": "text", "text": "role body"},
    ]);
    sqlx::query(
        "INSERT INTO system_prompts.session_prompts
            (session_id, request_id, lane, subagent_persona, blocks, content_sha256, altered)
         VALUES ($1, $2, 'subagent', NULL, $3, $4, false)",
    )
    .bind(session_id)
    .bind(request_id)
    .bind(&blocks)
    .bind(format!("versions-unresolved-marker-{run_marker}"))
    .execute(&pool)
    .await?;

    let resp = client
        .get(format!(
            "{inspect_url}/api/prompts/classes/subagent/unresolved/versions"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    assert_eq!(body["lane"].as_str(), Some("subagent"));
    assert!(body["subagent_persona"].is_null());
    let versions = body["versions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("versions must be an array: {body}"))?;
    let entry = versions
        .iter()
        .find(|v| v["version"].as_str() == Some("9.9.9"))
        .ok_or_else(|| anyhow::anyhow!("no 9.9.9 release in versions: {versions:?}"))?;
    assert_eq!(entry["observation_count"].as_i64(), Some(1));
    let models = entry["models"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("models must be an array: {entry}"))?;
    assert_eq!(
        models[0]["latest_request_id"].as_str(),
        Some(request_id.to_string().as_str())
    );

    let resp = client
        .get(format!(
            "{inspect_url}/api/prompts/classes/subagent/nonexistent-persona-{run_marker}/versions"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
