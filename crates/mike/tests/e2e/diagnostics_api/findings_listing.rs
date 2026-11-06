use crate::db_helpers;
use crate::upstream;

use db_helpers::start_proxy_with_db;
use serde_json::Value;

use super::support::{DiagInsert, insert_diagnostic_full, send_request, set_human_prompt};

async fn seed_five_diagnostics(pool: &sqlx::PgPool, req_id: i64) -> anyhow::Result<()> {
    let base = chrono::Utc::now();
    for (i, (cat, analyzer)) in [
        ("overselling", "an_a"),
        ("overselling", "an_b"),
        ("avoiding_work", "an_a"),
        ("avoiding_work", "an_b"),
        ("overselling", "an_a"),
    ]
    .iter()
    .enumerate()
    {
        let t = base + chrono::Duration::milliseconds(i as i64 * 10);
        let advice = format!("advice-{i}");
        let matched = format!("matched-{i}");
        insert_diagnostic_full(
            pool,
            DiagInsert {
                request_id: req_id,
                category: cat,
                analyzer,
                advice: &advice,
                matched_text: &matched,
                diag_created_at: t,
            },
        )
        .await?;
    }
    Ok(())
}

async fn assert_unfiltered_listing_is_desc_ordered(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let unfiltered: Value = client
        .get(format!("{inspect_url}/api/diagnostics/findings?limit=100"))
        .send()
        .await?
        .json()
        .await?;
    let unfiltered_arr = unfiltered
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    assert!(
        unfiltered_arr.len() >= 5,
        "expected at least 5 findings, got {}",
        unfiltered_arr.len()
    );
    for window in unfiltered_arr.windows(2) {
        let a = window[0]
            .get("created_at")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing"))?;
        let b = window[1]
            .get("created_at")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing"))?;
        assert!(a >= b, "DESC order broken {a} < {b}");
    }
    Ok(())
}

async fn assert_pagination_pages_do_not_overlap(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let p1: Value = client
        .get(format!(
            "{inspect_url}/api/diagnostics/findings?offset=0&limit=2"
        ))
        .send()
        .await?
        .json()
        .await?;
    let p2: Value = client
        .get(format!(
            "{inspect_url}/api/diagnostics/findings?offset=2&limit=2"
        ))
        .send()
        .await?
        .json()
        .await?;
    let extract_keys = |page: &Value| -> anyhow::Result<Vec<(u64, String)>> {
        let arr = page
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("not array"))?;
        Ok(arr
            .iter()
            .filter_map(|r| {
                let id = r.get("request_id").and_then(|v| v.as_u64())?;
                let cat = r.get("created_at").and_then(|v| v.as_str())?.to_string();
                Some((id, cat))
            })
            .collect())
    };
    let p1_keys = extract_keys(&p1)?;
    let p2_keys = extract_keys(&p2)?;
    for k in &p2_keys {
        assert!(!p1_keys.contains(k), "page1 and page2 overlap on {k:?}");
    }
    Ok(())
}

async fn assert_category_filter(client: &reqwest::Client, inspect_url: &str) -> anyhow::Result<()> {
    let by_cat: Value = client
        .get(format!(
            "{inspect_url}/api/diagnostics/findings?category=overselling&limit=100"
        ))
        .send()
        .await?
        .json()
        .await?;
    let by_cat_arr = by_cat
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    assert!(!by_cat_arr.is_empty());
    for row in by_cat_arr {
        let cat = row
            .get("category")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing category"))?;
        assert_eq!(cat, "overselling", "filter on category=overselling broken");
    }
    Ok(())
}

async fn assert_analyzer_filter(client: &reqwest::Client, inspect_url: &str) -> anyhow::Result<()> {
    let by_ana: Value = client
        .get(format!(
            "{inspect_url}/api/diagnostics/findings?analyzer=an_a&limit=100"
        ))
        .send()
        .await?
        .json()
        .await?;
    let by_ana_arr = by_ana
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    assert!(!by_ana_arr.is_empty());
    for row in by_ana_arr {
        let ana = row
            .get("analyzer")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing"))?;
        assert_eq!(ana, "an_a", "filter on analyzer=an_a broken");
    }
    Ok(())
}

#[tokio::test]
async fn findings_paginates_and_filters_by_category_and_analyzer() -> anyhow::Result<()> {
    let mock_server = upstream::canned_json_upstream("msg_a3_diagnostics").await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let session = uuid::Uuid::new_v4().to_string();
    send_request(&client, &proxy_url, &session, "u").await?;

    let row: (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT r.id, r.created_at FROM requests.requests r
          JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::uuid LIMIT 1",
    )
    .bind(&session)
    .fetch_one(&pool)
    .await?;
    let (req_id, _req_created_at) = row;

    seed_five_diagnostics(&pool, req_id).await?;
    assert_unfiltered_listing_is_desc_ordered(&client, &inspect_url).await?;
    assert_pagination_pages_do_not_overlap(&client, &inspect_url).await?;
    assert_category_filter(&client, &inspect_url).await?;
    assert_analyzer_filter(&client, &inspect_url).await?;

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn findings_total_true_returns_count_and_pagination_metadata() -> anyhow::Result<()> {
    let mock_server = upstream::canned_json_upstream("msg_a3_diagnostics").await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let session = uuid::Uuid::new_v4().to_string();
    send_request(&client, &proxy_url, &session, "u").await?;
    let row: (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT r.id, r.created_at FROM requests.requests r
          JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::uuid LIMIT 1",
    )
    .bind(&session)
    .fetch_one(&pool)
    .await?;
    let (req_id, _req_created_at) = row;

    let base = chrono::Utc::now();
    for i in 0..5_i64 {
        let t = base + chrono::Duration::milliseconds(i * 10);
        let analyzer = format!("an_{i}");
        let advice = format!("advice-{i}");
        let matched = format!("matched-{i}");
        insert_diagnostic_full(
            &pool,
            DiagInsert {
                request_id: req_id,
                category: "overselling",
                analyzer: &analyzer,
                advice: &advice,
                matched_text: &matched,
                diag_created_at: t,
            },
        )
        .await?;
    }

    let resp: Value = client
        .get(format!(
            "{inspect_url}/api/diagnostics/findings?total=true&offset=0&limit=2"
        ))
        .send()
        .await?
        .json()
        .await?;

    let findings = resp
        .get("findings")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing findings array"))?;
    let total = resp
        .get("total")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("missing total"))?;
    let offset = resp
        .get("offset")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("missing offset"))?;
    let limit = resp
        .get("limit")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("missing limit"))?;
    assert_eq!(findings.len(), 2, "page 1 must have 2 rows");
    assert!(total >= 5, "total must report >=5 rows, got {total}");
    assert_eq!(offset, 0);
    assert_eq!(limit, 2);

    let past_end: Value = client
        .get(format!(
            "{inspect_url}/api/diagnostics/findings?total=true&offset=10000&limit=10"
        ))
        .send()
        .await?
        .json()
        .await?;
    let past_findings = past_end
        .get("findings")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing findings"))?;
    let past_total = past_end
        .get("total")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("missing total"))?;
    assert!(past_findings.is_empty(), "off-the-end page must be empty");
    assert!(
        past_total >= 5,
        "total must still report the unpaged count even when the page is empty"
    );

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn findings_full_fidelity_strings_survive_unsliced() -> anyhow::Result<()> {
    let mock_server = upstream::canned_json_upstream("msg_a3_diagnostics").await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let session = uuid::Uuid::new_v4().to_string();
    send_request(&client, &proxy_url, &session, "u").await?;
    let row: (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT r.id, r.created_at FROM requests.requests r
          JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::uuid LIMIT 1",
    )
    .bind(&session)
    .fetch_one(&pool)
    .await?;
    let (req_id, _req_created_at) = row;

    let long_match: String = "match-".repeat(250);
    let long_advice: String = "advice-".repeat(220);
    let long_prompt: String = "prompt-".repeat(220);
    assert!(long_match.len() > 1000);
    assert!(long_advice.len() > 1000);
    assert!(long_prompt.len() > 1000);

    set_human_prompt(&pool, req_id, &long_prompt).await?;
    insert_diagnostic_full(
        &pool,
        DiagInsert {
            request_id: req_id,
            category: "overselling",
            analyzer: "fidelity_analyzer",
            advice: &long_advice,
            matched_text: &long_match,
            diag_created_at: chrono::Utc::now(),
        },
    )
    .await?;

    let resp: Value = client
        .get(format!(
            "{inspect_url}/api/diagnostics/findings?analyzer=fidelity_analyzer&limit=10"
        ))
        .send()
        .await?
        .json()
        .await?;
    let arr = resp
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    assert_eq!(
        arr.len(),
        1,
        "expected exactly 1 finding for fidelity_analyzer"
    );
    let finding = &arr[0];

    let matched_text = finding
        .get("matched_text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing matched_text"))?;
    let advice = finding
        .get("advice")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing advice"))?;
    let human_prompt = finding
        .get("request")
        .and_then(|r| r.get("human_prompt"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing request.human_prompt"))?;

    assert_eq!(
        matched_text, long_match,
        "matched_text must round-trip in full — no slicing"
    );
    assert_eq!(advice, long_advice, "advice must round-trip in full");
    assert_eq!(
        human_prompt, long_prompt,
        "request.human_prompt must round-trip in full"
    );

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
