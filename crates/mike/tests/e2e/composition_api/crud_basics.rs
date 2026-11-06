use crate::db_helpers;
use crate::upstream;

use serde_json::{Value, json};

use super::support::{create_via_api, prompt_sha256_for};

fn assert_post_roundtrip_envelope<'a>(
    envelope: &'a Value,
    prompt_sha256: &str,
) -> anyhow::Result<&'a str> {
    let composition = &envelope["composition"];
    let composition_id = composition["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("composition.id absent: {envelope}"))?;
    assert!(
        envelope["warnings"].as_array().is_some(),
        "warnings array required, got: {envelope}"
    );
    assert_eq!(composition["prompt_sha256"].as_str(), Some(prompt_sha256));
    assert_eq!(composition["label"].as_str(), Some("post-roundtrip"));
    assert_eq!(composition["version"].as_i64(), Some(1));
    assert_eq!(composition["applied_count_24h"].as_i64(), Some(0));
    let blocks = composition["blocks"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("blocks must be array: {envelope}"))?;
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[2]["text"].as_str(), Some("post-r1"));
    Ok(composition_id)
}

async fn assert_get_by_id_and_by_sha256_roundtrip(
    client: &reqwest::Client,
    inspect_url: &str,
    composition_id: &str,
    prompt_sha256: &str,
) -> anyhow::Result<()> {
    let by_id: Value = client
        .get(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(by_id["id"].as_str(), Some(composition_id));
    assert_eq!(by_id["prompt_sha256"].as_str(), Some(prompt_sha256));

    let by_class: Value = client
        .get(format!(
            "{inspect_url}/api/prompts/compositions/by-sha256/{prompt_sha256}"
        ))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(by_class["id"].as_str(), Some(composition_id));
    Ok(())
}

async fn assert_class_uniqueness_enforced(
    client: &reqwest::Client,
    inspect_url: &str,
    pool: &sqlx::PgPool,
    main_lane_composition_id: &str,
) -> anyhow::Result<()> {
    let dup = client
        .post(format!("{inspect_url}/api/prompts/compositions"))
        .json(&json!({
            "prompt_sha256": prompt_sha256_for("roundtrip-second-main-block2"),
            "label": "dup",
            "blocks": [{"text": "x"}, {"text": "y"}, {"text": "z"}]
        }))
        .send()
        .await?;
    assert_eq!(
        dup.status(),
        409,
        "a second main-lane registration must be refused — one manual main composition exists"
    );

    let other_class = client
        .post(format!("{inspect_url}/api/prompts/compositions"))
        .json(&json!({
            "prompt_sha256": prompt_sha256_for("roundtrip-subagent-block2"),
            "label": "dup-subagent",
            "agent_name": "roundtrip-subagent",
            "blocks": [{"text": "x"}, {"text": "y"}, {"text": "z"}]
        }))
        .send()
        .await?;
    assert_eq!(other_class.status(), 201);
    let other_envelope: Value = other_class.json().await?;
    let other_id = other_envelope["composition"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("subagent composition.id absent: {other_envelope}"))?;
    assert_ne!(
        other_id, main_lane_composition_id,
        "a distinct class must mint its own row, not return the main-lane one"
    );

    let main_lane_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM overrides.overrides WHERE lane = 'main' AND source_id = 1",
    )
    .fetch_one(pool)
    .await?;
    assert_eq!(
        main_lane_rows, 1,
        "exactly one manual main-lane override may exist; got {main_lane_rows}"
    );
    Ok(())
}

#[tokio::test]
async fn post_roundtrip_defaults_mode_observe() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let prompt_sha256 = prompt_sha256_for("post-roundtrip-block2");
    let envelope = create_via_api(
        &client,
        &inspect_url,
        &prompt_sha256,
        "post-roundtrip",
        "post-r1",
    )
    .await?;
    let composition_id = assert_post_roundtrip_envelope(&envelope, &prompt_sha256)?.to_string();

    assert_get_by_id_and_by_sha256_roundtrip(
        &client,
        &inspect_url,
        &composition_id,
        &prompt_sha256,
    )
    .await?;

    let stored_mode: (String,) = sqlx::query_as(
        "SELECT m.code FROM overrides.overrides o
         JOIN overrides.modes m ON m.id = o.mode_id
         WHERE o.prompt_sha256 = $1",
    )
    .bind(&prompt_sha256)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        stored_mode.0, "observe",
        "newly-created row must default to mode=observe"
    );

    assert_class_uniqueness_enforced(&client, &inspect_url, &pool, &composition_id).await?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn patch_unknown_id_returns_404() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let unknown = uuid::Uuid::new_v4().hyphenated().to_string();
    let resp = client
        .patch(format!("{inspect_url}/api/prompts/compositions/{unknown}"))
        .json(&json!({"label": "anything"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);
    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn delete_unknown_id_returns_404() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let unknown = uuid::Uuid::new_v4().hyphenated().to_string();
    let resp = client
        .delete(format!("{inspect_url}/api/prompts/compositions/{unknown}"))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);
    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn patch_notes_round_trips_set_and_clear() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let prompt_sha256 = prompt_sha256_for("patch-notes-block2");
    let envelope =
        create_via_api(&client, &inspect_url, &prompt_sha256, "patch-notes", "x").await?;
    let composition_id = envelope["composition"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("composition.id absent"))?
        .to_string();
    assert!(envelope["composition"]["notes"].is_null());

    let set_resp = client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .json(&json!({"notes": "operator note text"}))
        .send()
        .await?;
    assert_eq!(set_resp.status(), 200);
    let set_body: Value = set_resp.json().await?;
    assert_eq!(
        set_body["composition"]["notes"].as_str(),
        Some("operator note text"),
        "notes must round-trip on the wire"
    );

    let clear_resp = client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .json(&json!({"notes": Value::Null}))
        .send()
        .await?;
    assert_eq!(clear_resp.status(), 200);
    let clear_body: Value = clear_resp.json().await?;
    assert!(
        clear_body["composition"]["notes"].is_null(),
        "PATCH {{notes: null}} must clear the column; got {clear_body}",
    );

    let bump_resp = client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .json(&json!({"label": "patch-notes-bumped"}))
        .send()
        .await?;
    assert_eq!(bump_resp.status(), 200);
    let bump_body: Value = bump_resp.json().await?;
    assert!(
        bump_body["composition"]["notes"].is_null(),
        "PATCH that omits notes must not resurrect it"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
