use crate::db_helpers;
use serde_json::{Value, json};

use super::support::{fire_and_fetch_prompt_sha256, mock_upstream_canned, prompt_sha256_for};

#[tokio::test]
async fn cache_reload_after_patch() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let (proxy_url, inspect_url, shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();

    let post = client
        .post(format!("{inspect_url}/api/volatile-lines"))
        .json(&json!({
            "name": "cache-reload-patch",
            "pattern": "^cc_version=.*$",
            "applies_to_block_index": 2,
        }))
        .send()
        .await?;
    assert_eq!(post.status(), 201);
    let created: Value = post.json().await?;
    let pattern_id = created["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("id missing on POST: {created}"))?
        .to_string();

    let session_a = "33333333-aaaa-bbbb-cccc-111111111111";
    let block2_a = "cc_version=v1\nrole-body-content";
    let sha_baseline =
        fire_and_fetch_prompt_sha256(&client, &proxy_url, &inspect_url, session_a, block2_a)
            .await?
            .ok_or_else(|| anyhow::anyhow!("baseline sha missing"))?;
    assert_eq!(sha_baseline, prompt_sha256_for("role-body-content"));

    let patch = client
        .patch(format!("{inspect_url}/api/volatile-lines/{pattern_id}"))
        .json(&json!({"pattern": "^never-matches-anything=.*$"}))
        .send()
        .await?;
    assert_eq!(patch.status(), 200);

    let session_b = "33333333-aaaa-bbbb-cccc-222222222222";
    let sha_after_patch =
        fire_and_fetch_prompt_sha256(&client, &proxy_url, &inspect_url, session_b, block2_a)
            .await?
            .ok_or_else(|| anyhow::anyhow!("post-patch sha missing"))?;
    assert_ne!(
        sha_baseline, sha_after_patch,
        "cache must reload after PATCH so the new pattern (which doesn't match) takes effect"
    );
    assert_eq!(sha_after_patch, prompt_sha256_for(block2_a));

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn cache_reload_after_delete() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let (proxy_url, inspect_url, shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();

    let post = client
        .post(format!("{inspect_url}/api/volatile-lines"))
        .json(&json!({
            "name": "cache-reload-delete",
            "pattern": "^cc_version=.*$",
            "applies_to_block_index": 2,
        }))
        .send()
        .await?;
    assert_eq!(post.status(), 201);
    let created: Value = post.json().await?;
    let pattern_id = created["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("id missing on POST: {created}"))?
        .to_string();

    let block2 = "cc_version=v1\nrole-body-content";

    let session_a = "44444444-aaaa-bbbb-cccc-111111111111";
    let sha_with_pattern =
        fire_and_fetch_prompt_sha256(&client, &proxy_url, &inspect_url, session_a, block2)
            .await?
            .ok_or_else(|| anyhow::anyhow!("with-pattern sha missing"))?;
    assert_eq!(sha_with_pattern, prompt_sha256_for("role-body-content"));

    let del = client
        .delete(format!("{inspect_url}/api/volatile-lines/{pattern_id}"))
        .send()
        .await?;
    assert_eq!(del.status(), 204);

    let session_b = "44444444-aaaa-bbbb-cccc-222222222222";
    let sha_after_delete =
        fire_and_fetch_prompt_sha256(&client, &proxy_url, &inspect_url, session_b, block2)
            .await?
            .ok_or_else(|| anyhow::anyhow!("post-delete sha missing"))?;
    assert_ne!(
        sha_with_pattern, sha_after_delete,
        "cache must reload after DELETE so classification reverts to unstripped form"
    );
    assert_eq!(sha_after_delete, prompt_sha256_for(block2));

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
