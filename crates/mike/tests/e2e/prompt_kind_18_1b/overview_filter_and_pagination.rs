use crate::db_helpers;
use serde_json::Value;
use sqlx::PgPool;

use super::support::{
    InsertRequestParams, InsertTurnParams, bootstrap_project_session, insert_classification,
    insert_request, insert_response, insert_text_chunk, insert_turn,
};

async fn seed_four_mixed_kind_turns(
    pool: &PgPool,
    project_id: i64,
    session_id: &str,
) -> anyhow::Result<()> {
    let prompts = [
        ("implement the feature", "operator"),
        (
            "Describe your most recent action in 3-5 words: writing tests",
            "activity-label",
        ),
        ("Stop hook feedback: lint failed", "hook-feedback"),
        ("refactor the module", "operator"),
    ];

    for (i, (prompt, kind)) in prompts.iter().enumerate() {
        let r = insert_request(
            pool,
            InsertRequestParams {
                session_id,
                project_id,
                turn_origin: "primary",
                turn_type: "user_prompt",
                status: "complete",
                human_prompt: prompt,
                completed: true,
                created_at_offset_ms: (i as i64) * 1_000,
            },
        )
        .await?;
        insert_response(pool, r, "end_turn").await?;
        insert_text_chunk(pool, r, session_id, 0, &format!("response to: {prompt}")).await?;
        insert_turn(
            pool,
            InsertTurnParams {
                session_id,
                project_id,
                start_request_id: r,
                end_request_id: r,
                user_prompt: Some(prompt),
                outcome: "accepted",
                closed: true,
            },
        )
        .await?;
        insert_classification(pool, r, session_id, kind).await?;
    }
    Ok(())
}

async fn assert_default_call_full_fidelity(
    client: &reqwest::Client,
    base_url: &str,
) -> anyhow::Result<()> {
    let default_resp: Value = client.get(base_url).send().await?.json().await?;
    assert_eq!(
        default_resp["total_turns"], 4,
        "default: should see all 4 turns"
    );
    let turns = default_resp["turns"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing turns array: {default_resp}"))?;
    assert_eq!(turns.len(), 4, "default: 4 turns in array");
    for t in turns {
        assert!(
            t["prompt_kind"].is_string(),
            "every turn must carry prompt_kind in default call: {t}"
        );
        assert!(
            t["stop_response"].is_string(),
            "stop_response must be present in default call (full-fidelity): {t}"
        );
    }
    Ok(())
}

async fn assert_operator_filter_returns_two(
    client: &reqwest::Client,
    base_url: &str,
) -> anyhow::Result<()> {
    let filtered: Value = client
        .get(format!("{base_url}?prompt_kind=operator"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        filtered["total_turns"], 4,
        "total_turns always reflects the unfiltered session total"
    );
    let filtered_turns = filtered["turns"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing turns array: {filtered}"))?;
    assert_eq!(
        filtered_turns.len(),
        2,
        "filter=operator should return 2 turns, got: {filtered}"
    );
    for t in filtered_turns {
        assert_eq!(
            t["prompt_kind"], "operator",
            "filtered turns must all be operator kind: {t}"
        );
    }
    Ok(())
}

async fn assert_pagination_pages_dont_overlap(
    client: &reqwest::Client,
    base_url: &str,
) -> anyhow::Result<()> {
    let page1: Value = client
        .get(format!("{base_url}?limit=2"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        page1["total_turns"], 4,
        "paginated: total_turns is unaffected"
    );
    let page1_turns = page1["turns"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing turns: {page1}"))?;
    assert_eq!(page1_turns.len(), 2, "limit=2 must return 2 turns");
    assert_eq!(page1["limit"], 2, "limit reflected in envelope: {page1}");
    assert_eq!(page1["offset"], 0, "offset default 0 in envelope: {page1}");

    let page2: Value = client
        .get(format!("{base_url}?limit=2&offset=2"))
        .send()
        .await?
        .json()
        .await?;
    let page2_turns = page2["turns"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing turns: {page2}"))?;
    assert_eq!(page2_turns.len(), 2, "second page must return 2 turns");
    assert_eq!(page2["offset"], 2, "offset reflected in envelope");
    let page1_ids: Vec<&Value> = page1_turns.iter().collect();
    let page2_ids: Vec<&Value> = page2_turns.iter().collect();
    for p2_turn in &page2_ids {
        for p1_turn in &page1_ids {
            assert_ne!(
                p2_turn["turn_id"], p1_turn["turn_id"],
                "pages must not overlap"
            );
        }
    }
    Ok(())
}

async fn assert_fields_prompts_drops_stop_response(
    client: &reqwest::Client,
    base_url: &str,
) -> anyhow::Result<()> {
    let fields_resp: Value = client
        .get(format!("{base_url}?fields=prompts"))
        .send()
        .await?
        .json()
        .await?;
    let fields_turns = fields_resp["turns"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing turns: {fields_resp}"))?;
    assert_eq!(fields_turns.len(), 4, "fields=prompts: all 4 turns");
    for t in fields_turns {
        assert!(
            t["stop_response"].is_null()
                || !t
                    .get("stop_response")
                    .map(|v| v.is_string())
                    .unwrap_or(false),
            "fields=prompts must omit stop_response: {t}"
        );
        assert!(
            t["prompt_kind"].is_string(),
            "fields=prompts: prompt_kind must still be present: {t}"
        );
        assert!(
            t["user_prompt"].is_string(),
            "fields=prompts: user_prompt must still be present: {t}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn overview_prompt_kind_filter_and_pagination() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "2a000000-0000-4000-8000-000000000001";
    let project_id = bootstrap_project_session(&pool, "pk-overview-filter", session_id).await?;
    seed_four_mixed_kind_turns(&pool, project_id, session_id).await?;

    let client = reqwest::Client::new();
    let base_url = format!("{inspect_url}/api/sessions/{session_id}/overview");

    assert_default_call_full_fidelity(&client, &base_url).await?;
    assert_operator_filter_returns_two(&client, &base_url).await?;
    assert_pagination_pages_dont_overlap(&client, &base_url).await?;
    assert_fields_prompts_drops_stop_response(&client, &base_url).await?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
