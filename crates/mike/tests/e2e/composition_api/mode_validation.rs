use crate::db_helpers;
use crate::upstream;

use serde_json::{Value, json};

use super::support::{create_subagent_via_api, create_via_api, prompt_sha256_for};

#[tokio::test]
async fn post_with_mode_template_persists_mode() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let prompt_sha256 = prompt_sha256_for("post-mode-template-block2");
    let resp = client
        .post(format!("{inspect_url}/api/prompts/compositions"))
        .json(&json!({
            "prompt_sha256": prompt_sha256,
            "label": "post-mode-template",
            "mode": "template",
            "blocks": [
                {"text": "billing-header"},
                {"text": "identity preamble"},
                {"text": "REWRITTEN-AT-CREATE"},
            ]
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 201);
    let body: Value = resp.json().await?;
    assert_eq!(
        body["composition"]["mode"].as_str(),
        Some("template"),
        "POST {{mode: template}} must round-trip on the wire shape"
    );

    let db_mode: String = sqlx::query_scalar(
        "SELECT m.code FROM overrides.overrides o
         JOIN overrides.modes m ON m.id = o.mode_id
         WHERE o.prompt_sha256 = $1",
    )
    .bind(&prompt_sha256)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        db_mode, "template",
        "DB row mode must equal the mode the POST body specified"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn post_without_mode_defaults_to_observe() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let prompt_sha256 = prompt_sha256_for("post-mode-default-block2");
    let resp = client
        .post(format!("{inspect_url}/api/prompts/compositions"))
        .json(&json!({
            "prompt_sha256": prompt_sha256,
            "label": "post-mode-default",
            "blocks": [
                {"text": "billing-header"},
                {"text": "identity preamble"},
                {"text": "noop"},
            ]
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 201);
    let body: Value = resp.json().await?;
    assert_eq!(
        body["composition"]["mode"].as_str(),
        Some("observe"),
        "missing-mode POST must default to observe"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn create_with_replace_mode_returns_400() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let prompt_sha256 = prompt_sha256_for("post-mode-replace-block2");
    let resp = client
        .post(format!("{inspect_url}/api/prompts/compositions"))
        .json(&json!({
            "prompt_sha256": prompt_sha256,
            "label": "post-mode-replace",
            "mode": "replace",
            "blocks": [
                {"text": "billing-header"},
                {"text": "identity preamble"},
                {"text": "noop"},
            ]
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await?;
    let err = body["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error string missing: {body}"))?;
    assert!(
        err.contains("replace mode is not supported"),
        "400 body must mention V1 rejection of replace, got: {err}"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM overrides.overrides WHERE prompt_sha256 = $1",
    )
    .bind(&prompt_sha256)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        count, 0,
        "replace mode must reject before insert (no DB row)"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn patch_with_replace_mode_returns_400() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let prompt_sha256 = prompt_sha256_for("patch-mode-replace-block2");
    let envelope = create_via_api(
        &client,
        &inspect_url,
        &prompt_sha256,
        "patch-mode-replace",
        "x",
    )
    .await?;
    let composition_id = envelope["composition"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("composition.id absent"))?
        .to_string();

    let resp = client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .json(&json!({"mode": "replace"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await?;
    let err = body["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error string missing: {body}"))?;
    assert!(
        err.contains("replace mode is not supported"),
        "PATCH 400 body must mention V1 rejection of replace, got: {err}"
    );

    let stored_mode: String = sqlx::query_scalar(
        "SELECT m.code FROM overrides.overrides o
         JOIN overrides.modes m ON m.id = o.mode_id
         WHERE o.prompt_sha256 = $1",
    )
    .bind(&prompt_sha256)
    .fetch_one(&pool)
    .await?;
    assert_eq!(stored_mode, "observe");

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn create_subagent_composition_with_template_mode_returns_400() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let prompt_sha256 = prompt_sha256_for("post-subagent-template-block2");
    let resp = client
        .post(format!("{inspect_url}/api/prompts/compositions"))
        .json(&json!({
            "prompt_sha256": prompt_sha256,
            "label": "post-subagent-template",
            "mode": "template",
            "agent_name": "rekey-test-subagent-a",
            "blocks": [
                {"text": "billing-header"},
                {"text": "identity preamble"},
                {"text": "noop"},
            ]
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await?;
    let err = body["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error string missing: {body}"))?;
    assert!(
        err.contains("observe"),
        "400 body must explain the subagent lane is observe-only, got: {err}"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM overrides.overrides WHERE prompt_sha256 = $1",
    )
    .bind(&prompt_sha256)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        count, 0,
        "subagent lane + template mode must reject before insert (no DB row)"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn patch_subagent_composition_to_template_mode_returns_400() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let prompt_sha256 = prompt_sha256_for("patch-subagent-template-block2");
    let envelope = create_subagent_via_api(
        &client,
        &inspect_url,
        &prompt_sha256,
        "patch-subagent-template",
        "x",
        "rekey-test-subagent-b",
    )
    .await?;
    let composition_id = envelope["composition"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("composition.id absent"))?
        .to_string();

    let resp = client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .json(&json!({"mode": "template"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await?;
    let err = body["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error string missing: {body}"))?;
    assert!(
        err.contains("observe"),
        "PATCH 400 body must explain the subagent lane is observe-only, got: {err}"
    );

    let stored_mode: String = sqlx::query_scalar(
        "SELECT m.code FROM overrides.overrides o
         JOIN overrides.modes m ON m.id = o.mode_id
         WHERE o.prompt_sha256 = $1",
    )
    .bind(&prompt_sha256)
    .fetch_one(&pool)
    .await?;
    assert_eq!(stored_mode, "observe");

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn create_subagent_composition_with_observe_mode_succeeds() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let prompt_sha256 = prompt_sha256_for("post-subagent-observe-block2");
    let resp = client
        .post(format!("{inspect_url}/api/prompts/compositions"))
        .json(&json!({
            "prompt_sha256": prompt_sha256,
            "label": "post-subagent-observe",
            "mode": "observe",
            "agent_name": "rekey-test-subagent-c",
            "blocks": [
                {"text": "billing-header"},
                {"text": "identity preamble"},
                {"text": "noop"},
            ]
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 201);
    let body: Value = resp.json().await?;
    assert_eq!(body["composition"]["mode"].as_str(), Some("observe"));

    let stored_lane: String =
        sqlx::query_scalar("SELECT lane FROM overrides.overrides WHERE prompt_sha256 = $1")
            .bind(&prompt_sha256)
            .fetch_one(&pool)
            .await?;
    assert_eq!(stored_lane, "subagent");

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn post_with_invalid_mode_returns_400() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let prompt_sha256 = prompt_sha256_for("post-bad-mode-block2");
    let resp = client
        .post(format!("{inspect_url}/api/prompts/compositions"))
        .json(&json!({
            "prompt_sha256": prompt_sha256,
            "label": "post-bad-mode",
            "mode": "definitely-not-a-mode",
            "blocks": [
                {"text": "billing-header"},
                {"text": "identity preamble"},
                {"text": "noop"},
            ]
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await?;
    let err = body["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error string missing: {body}"))?;
    assert!(
        err.contains("observe") && err.contains("template") && err.contains("replace"),
        "400 body must enumerate the valid modes; got {err}"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM overrides.overrides WHERE prompt_sha256 = $1",
    )
    .bind(&prompt_sha256)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        count, 0,
        "validation failure must reject before insert (DB row must NOT exist)"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
