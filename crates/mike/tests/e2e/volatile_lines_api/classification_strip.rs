use crate::db_helpers;
use serde_json::json;

use super::support::{fire_and_fetch_prompt_sha256, mock_upstream_canned, prompt_sha256_for};

#[tokio::test]
async fn pattern_strips_lines_before_classification() -> anyhow::Result<()> {
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
            "name": "cc-version-strip",
            "pattern": "^cc_version=.*$",
            "applies_to_block_index": 2,
        }))
        .send()
        .await?;
    assert_eq!(post.status(), 201);

    let with_volatile = "cc_version=abc; cch=def\nrole body content";
    let without_volatile = "role body content";

    let session_a = "11111111-2222-3333-4444-aaaaaaaaaaaa";
    let session_b = "11111111-2222-3333-4444-bbbbbbbbbbbb";

    let sha_a =
        fire_and_fetch_prompt_sha256(&client, &proxy_url, &inspect_url, session_a, with_volatile)
            .await?;
    let sha_b = fire_and_fetch_prompt_sha256(
        &client,
        &proxy_url,
        &inspect_url,
        session_b,
        without_volatile,
    )
    .await?;

    let sha_a = sha_a.ok_or_else(|| anyhow::anyhow!("sha_a missing"))?;
    let sha_b = sha_b.ok_or_else(|| anyhow::anyhow!("sha_b missing"))?;
    assert_eq!(
        sha_a, sha_b,
        "with the volatile pattern enabled, the two block-2 texts must produce the SAME prompt_sha256"
    );
    let expected = prompt_sha256_for(without_volatile);
    assert_eq!(
        sha_a, expected,
        "stripped hash must equal sha256 of the line that survives the strip"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn disabled_pattern_does_not_affect_classification() -> anyhow::Result<()> {
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
            "name": "cc-version-disabled",
            "pattern": "^cc_version=.*$",
            "applies_to_block_index": 2,
            "enabled": false,
        }))
        .send()
        .await?;
    assert_eq!(post.status(), 201);

    let with_volatile = "cc_version=abc; cch=def\nrole body content";
    let without_volatile = "role body content";

    let session_a = "22222222-2222-3333-4444-aaaaaaaaaaaa";
    let session_b = "22222222-2222-3333-4444-bbbbbbbbbbbb";

    let sha_a =
        fire_and_fetch_prompt_sha256(&client, &proxy_url, &inspect_url, session_a, with_volatile)
            .await?;
    let sha_b = fire_and_fetch_prompt_sha256(
        &client,
        &proxy_url,
        &inspect_url,
        session_b,
        without_volatile,
    )
    .await?;

    let sha_a = sha_a.ok_or_else(|| anyhow::anyhow!("sha_a missing"))?;
    let sha_b = sha_b.ok_or_else(|| anyhow::anyhow!("sha_b missing"))?;
    assert_ne!(
        sha_a, sha_b,
        "with the volatile pattern DISABLED, the two block-2 texts must produce DIFFERENT prompt_sha256"
    );
    assert_eq!(sha_a, prompt_sha256_for(with_volatile));
    assert_eq!(sha_b, prompt_sha256_for(without_volatile));

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
