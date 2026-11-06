use std::collections::HashMap;

use crate::db_helpers;

use serde_json::{Value, json};
use sqlx::PgPool;

use super::fixtures::{
    fetch_anomalies, insert_request_row, mock_upstream_canned, prompt_sha256_for,
};

async fn seed_unknown_class_shas(pool: &PgPool) -> anyhow::Result<Vec<String>> {
    let shas: Vec<String> = (0..3)
        .map(|i| prompt_sha256_for(&format!("agg-novel-class-{i}")))
        .collect();
    for (i, sha) in shas.iter().enumerate() {
        let blocks = json!([
            {"type": "text", "text": "billing"},
            {"type": "text", "text": "preamble"},
            {"type": "text", "text": format!("agg-novel-class-{i}")},
        ]);
        sqlx::query(
            "INSERT INTO system_prompts.system_prompts (prompt_sha256, blocks, first_seen_at, last_seen_at)
             SELECT $1, $2, NOW(), NOW()
             WHERE NOT EXISTS (
                 SELECT 1 FROM system_prompts.system_prompts WHERE prompt_sha256 = $1
             )",
        )
        .bind(sha)
        .bind(&blocks)
        .execute(pool)
        .await?;
    }
    Ok(shas)
}

async fn seed_novel_class_requests(
    pool: &PgPool,
    shas: &[String],
    mut next_id: i64,
    expected_session_for_request: &mut HashMap<i64, &'static str>,
) -> anyhow::Result<i64> {
    const NOVEL_SESSIONS: [&str; 3] = [
        "a0000000-0000-4000-8000-000000000001",
        "a0000000-0000-4000-8000-000000000002",
        "a0000000-0000-4000-8000-000000000003",
    ];
    for (sha_idx, sha) in shas.iter().enumerate() {
        let blocks = json!([
            {"type": "text", "text": "billing"},
            {"type": "text", "text": "preamble"},
            {"type": "text", "text": format!("agg-novel-class-{sha_idx}")},
        ]);
        for occurrence in 0..20 {
            let session_id = NOVEL_SESSIONS[occurrence % 3];
            insert_request_row(pool, session_id, next_id, Some(&blocks), Some(sha), None).await?;
            expected_session_for_request.insert(next_id, session_id);
            next_id += 1;
        }
    }
    Ok(next_id)
}

async fn seed_structural_mismatch_requests(
    pool: &PgPool,
    mut next_id: i64,
    expected_session_for_request: &mut HashMap<i64, &'static str>,
) -> anyhow::Result<i64> {
    let structural_payloads = [
        json!({"reason": "non_3_block", "len": 2}),
        json!({"reason": "non_3_block", "len": 4}),
    ];
    const STRUCTURAL_SESSIONS: [&str; 4] = [
        "b0000000-0000-4000-8000-000000000001",
        "b0000000-0000-4000-8000-000000000002",
        "b0000000-0000-4000-8000-000000000003",
        "b0000000-0000-4000-8000-000000000004",
    ];
    for payload in &structural_payloads {
        for occurrence in 0..20 {
            let session_id = STRUCTURAL_SESSIONS[occurrence % 4];
            insert_request_row(pool, session_id, next_id, None, None, Some(payload)).await?;
            expected_session_for_request.insert(next_id, session_id);
            next_id += 1;
        }
    }
    Ok(next_id)
}

fn assert_group_aggregation(
    anomalies: &[Value],
    expected_session_for_request: &HashMap<i64, &'static str>,
) -> anyhow::Result<()> {
    assert_eq!(
        anomalies.len(),
        5,
        "expected 5 groups (3 unknown-class + 2 structural-mismatch); got {anomalies:?}"
    );

    let mut total_count: i64 = 0;
    let mut total_observations: usize = 0;
    for group in anomalies {
        let count = group["count"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("count must be int; got {group}"))?;
        assert_eq!(count, 20, "every group must aggregate 20 occurrences");
        total_count += count;

        let observations = group["observations"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("observations must be array"))?;
        assert_eq!(
            observations.len(),
            20,
            "observations list must hold every occurrence (no slicing)"
        );
        total_observations += observations.len();

        for obs in observations {
            let request_id = obs["request_id"]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("observation request_id must be int; got {obs}"))?;
            let session_id = obs["session_id"].as_str().ok_or_else(|| {
                anyhow::anyhow!("observation session_id must be string; got {obs}")
            })?;
            let expected = expected_session_for_request
                .get(&request_id)
                .ok_or_else(|| {
                    anyhow::anyhow!("observation names a request_id never inserted: {request_id}")
                })?;
            assert_eq!(
                session_id, *expected,
                "observation for request {request_id} must carry ITS OWN session, not \
                 another request's — got {session_id}, wanted {expected}"
            );
        }
    }
    assert_eq!(total_count, 100);
    assert_eq!(total_observations, 100);
    Ok(())
}

#[tokio::test]
async fn anomalies_groups_repeating_patterns_into_single_rows() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let shas = seed_unknown_class_shas(&pool).await?;

    let mut expected_session_for_request: HashMap<i64, &'static str> = HashMap::new();
    let next_id =
        seed_novel_class_requests(&pool, &shas, 1_000, &mut expected_session_for_request).await?;
    seed_structural_mismatch_requests(&pool, next_id, &mut expected_session_for_request).await?;

    let anomalies = fetch_anomalies(&client, &inspect_url).await?;
    assert_group_aggregation(&anomalies, &expected_session_for_request)?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
