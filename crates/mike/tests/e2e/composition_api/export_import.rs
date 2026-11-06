use crate::db_helpers;
use crate::upstream;

use serde_json::{Value, json};

use super::support::{create_subagent_via_api, prompt_sha256_for};

#[tokio::test]
async fn export_returns_envelope_with_seeded_compositions() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let baseline_count = client
        .get(format!("{inspect_url}/api/prompts/compositions/export"))
        .send()
        .await?
        .json::<Value>()
        .await?["compositions"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    let sha_a = prompt_sha256_for("export-a-block2");
    let sha_b = prompt_sha256_for("export-b-block2");
    let env_a = client
        .post(format!("{inspect_url}/api/prompts/compositions"))
        .json(&json!({
            "prompt_sha256": sha_a,
            "label": "export-a",
            "mode": "template",
            "blocks": [
                {"text": "billing-header"},
                {"text": "identity preamble"},
                {"text": "TXT-A"},
            ]
        }))
        .send()
        .await?;
    assert_eq!(env_a.status(), 201);
    create_subagent_via_api(
        &client,
        &inspect_url,
        &sha_b,
        "export-b",
        "TXT-B",
        "export-b-agent",
    )
    .await?;

    let resp = client
        .get(format!("{inspect_url}/api/prompts/compositions/export"))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    assert_eq!(body["version"].as_u64(), Some(1));
    let compositions = body["compositions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("compositions array missing: {body}"))?;
    assert_eq!(compositions.len(), baseline_count + 2);

    let mut shas: Vec<String> = compositions
        .iter()
        .map(|c| c["prompt_sha256"].as_str().unwrap_or("").to_string())
        .collect();
    let mut sorted = shas.clone();
    sorted.sort();
    assert_eq!(shas, sorted, "export rows must be sorted by prompt_sha256");
    shas.sort();
    assert!(
        shas.contains(&sha_a) && shas.contains(&sha_b),
        "export must include both created compositions (sha_a, sha_b); got {shas:?}"
    );

    let template_entry = compositions
        .iter()
        .find(|c| c["prompt_sha256"].as_str() == Some(sha_a.as_str()))
        .ok_or_else(|| anyhow::anyhow!("export missing the sha-A entry"))?;
    assert_eq!(template_entry["mode"].as_str(), Some("template"));

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

fn build_import_payload(sha_new1: &str, sha_new2: &str, sha_existing: &str) -> Value {
    json!({
        "version": 1,
        "compositions": [
            {
                "prompt_sha256": sha_new1,
                "label": "import-new1",
                "agent_name": "import-new1-agent",
                "mode": "template",
                "blocks": [
                    {"text": "billing-header"},
                    {"text": "identity preamble"},
                    {"text": "NEW1"},
                ]
            },
            {
                "prompt_sha256": sha_new2,
                "label": "import-new2",
                "agent_name": "import-new2-agent",
                "mode": "observe",
                "blocks": [
                    {"text": "billing-header"},
                    {"text": "identity preamble"},
                    {"text": "NEW2"},
                ]
            },
            {
                "prompt_sha256": sha_existing,
                "label": "import-existing-redux",
                "mode": "observe",
                "blocks": [
                    {"text": "billing-header"},
                    {"text": "identity preamble"},
                    {"text": "DOES-NOT-OVERWRITE"},
                ]
            },
            {
                "prompt_sha256": prompt_sha256_for("import-bad-mode-block2"),
                "label": "import-bad-mode",
                "mode": "definitely-not-a-mode",
                "blocks": [
                    {"text": "billing-header"},
                    {"text": "identity preamble"},
                    {"text": "BAD-MODE"},
                ]
            }
        ]
    })
}

fn assert_import_row_statuses(
    rows: &[Value],
    sha_new1: &str,
    sha_new2: &str,
    sha_existing: &str,
) -> anyhow::Result<()> {
    assert_eq!(rows.len(), 4);
    let allowed_statuses = ["imported", "skipped", "error"];
    let expected_input_order = [sha_new1, sha_new2, sha_existing];
    for (i, row) in rows.iter().enumerate() {
        let status = row["status"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("row {i} missing status: {row}"))?;
        assert!(
            allowed_statuses.contains(&status),
            "row {i} status {status:?} must be one of {allowed_statuses:?}"
        );
        let row_sha = row["prompt_sha256"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("row {i} missing prompt_sha256: {row}"))?;
        if let Some(expected_sha) = expected_input_order.get(i) {
            assert_eq!(
                row_sha, *expected_sha,
                "row {i} prompt_sha256 must round-trip in the input payload's order"
            );
        }
    }
    assert_eq!(rows[0]["status"].as_str(), Some("imported"));
    assert_eq!(rows[1]["status"].as_str(), Some("imported"));
    assert_eq!(rows[2]["status"].as_str(), Some("skipped"));
    assert_eq!(rows[3]["status"].as_str(), Some("error"));
    assert_eq!(
        rows[3]["prompt_sha256"].as_str(),
        Some(prompt_sha256_for("import-bad-mode-block2").as_str()),
        "the 4th row's prompt_sha256 must echo back the bad-mode entry's sha"
    );
    Ok(())
}

async fn assert_import_db_state(
    pool: &sqlx::PgPool,
    sha_existing: &str,
    sha_new1: &str,
    sha_new2: &str,
) -> anyhow::Result<()> {
    let existing_labels: Vec<String> = sqlx::query_scalar(
        "SELECT label FROM overrides.overrides WHERE prompt_sha256 = $1 ORDER BY id ASC",
    )
    .bind(sha_existing)
    .fetch_all(pool)
    .await?;
    assert_eq!(
        existing_labels,
        vec!["import-existing".to_string()],
        "a skipped import must leave the existing row untouched and add nothing — \
         got {existing_labels:?}"
    );

    let new1_mode: String = sqlx::query_scalar(
        "SELECT m.code FROM overrides.overrides o
         JOIN overrides.modes m ON m.id = o.mode_id
         WHERE o.prompt_sha256 = $1",
    )
    .bind(sha_new1)
    .fetch_one(pool)
    .await?;
    assert_eq!(new1_mode, "template");
    let new2_mode: String = sqlx::query_scalar(
        "SELECT m.code FROM overrides.overrides o
         JOIN overrides.modes m ON m.id = o.mode_id
         WHERE o.prompt_sha256 = $1",
    )
    .bind(sha_new2)
    .fetch_one(pool)
    .await?;
    assert_eq!(new2_mode, "observe");
    Ok(())
}

#[tokio::test]
async fn import_inserts_new_rows_and_skips_existing() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let sha_existing = prompt_sha256_for("import-existing-block2");
    super::support::create_via_api(
        &client,
        &inspect_url,
        &sha_existing,
        "import-existing",
        "OLD",
    )
    .await?;

    let sha_new1 = prompt_sha256_for("import-new1-block2");
    let sha_new2 = prompt_sha256_for("import-new2-block2");
    let payload = build_import_payload(&sha_new1, &sha_new2, &sha_existing);

    let resp = client
        .post(format!("{inspect_url}/api/prompts/compositions/import"))
        .json(&payload)
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    assert_eq!(body["imported"].as_u64(), Some(2));
    assert_eq!(body["skipped"].as_u64(), Some(1));
    assert_eq!(body["errors"].as_u64(), Some(1));

    let rows = body["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("rows array missing: {body}"))?;
    assert_import_row_statuses(rows, &sha_new1, &sha_new2, &sha_existing)?;
    assert_import_db_state(&pool, &sha_existing, &sha_new1, &sha_new2).await?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn import_rejects_unsupported_version_with_400() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{inspect_url}/api/prompts/compositions/import"))
        .json(&json!({
            "version": 999,
            "compositions": []
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await?;
    let err = body["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error missing: {body}"))?;
    assert!(err.contains("999") && err.contains("expected 1"));

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
