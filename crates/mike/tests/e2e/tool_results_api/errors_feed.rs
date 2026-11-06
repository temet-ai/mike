use crate::db_helpers;
use serde_json::{Value, json};

use super::support::{
    follow_up_body_with_tool_result, insert_emitting_request, start_upstream_stub,
};

#[tokio::test]
async fn list_tool_errors_surfaces_is_error_results_with_tool_name_join() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-446655440003";
    let project_path = "test-project-errors-feed";
    let tool_use_id = "toolu_err_001";

    let emitting_id =
        insert_emitting_request(&pool, session_id, project_path, tool_use_id, "Edit", 0).await?;

    let follow_up = follow_up_body_with_tool_result(
        session_id,
        tool_use_id,
        true,
        json!("error: file not found"),
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("content-type", "application/json")
        .json(&follow_up)
        .send()
        .await?;
    assert!(resp.status().is_success());

    let resp = client
        .get(format!("{inspect_url}/api/tool-results/errors"))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    let rows = body["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("rows must be an array"))?;
    let target = rows
        .iter()
        .find(|r| r["tool_use_id"].as_str() == Some(tool_use_id))
        .ok_or_else(|| anyhow::anyhow!("error row missing for tool_use_id"))?;
    assert_eq!(target["request_id"].as_i64(), Some(emitting_id));
    assert_eq!(
        target["tool_name"].as_str(),
        Some("Edit"),
        "tool_uses_v join must surface the originating tool_name"
    );
    assert_eq!(target["session_id"].as_str(), Some(session_id));
    assert_eq!(target["project"].as_str(), Some(project_path));
    assert_eq!(
        target["content"],
        Value::String("error: file not found".to_string())
    );

    let resp = client
        .get(format!(
            "{inspect_url}/api/tool-results/errors?tool_name=Edit"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    let rows = body["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("rows must be an array"))?;
    assert!(
        rows.iter()
            .any(|r| r["tool_use_id"].as_str() == Some(tool_use_id)),
        "tool_name=Edit filter must keep matching rows"
    );

    let resp = client
        .get(format!(
            "{inspect_url}/api/tool-results/errors?tool_name=Read"
        ))
        .send()
        .await?;
    let body: Value = resp.json().await?;
    let rows = body["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("rows must be an array"))?;
    assert!(
        !rows
            .iter()
            .any(|r| r["tool_use_id"].as_str() == Some(tool_use_id)),
        "tool_name=Read filter must exclude non-matching rows"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

async fn seed_two_project_error_rows(
    client: &reqwest::Client,
    proxy_url: &str,
    pool: &sqlx::PgPool,
) -> anyhow::Result<(i64, i64, &'static str)> {
    let session_a = "550e8400-e29b-41d4-a716-446655440010";
    let session_b = "550e8400-e29b-41d4-a716-446655440011";
    let project_a = "am-night-watch/tools/pipeline-utilities-214a";
    let project_b = "temet/temet-proxy-214b";
    let tool_use_a = "toolu_proj_filter_a";
    let tool_use_b = "toolu_proj_filter_b";

    let emitting_a =
        insert_emitting_request(pool, session_a, project_a, tool_use_a, "Bash", 0).await?;
    let emitting_b =
        insert_emitting_request(pool, session_b, project_b, tool_use_b, "Bash", 1).await?;

    for (session_id, tool_use_id) in [(session_a, tool_use_a), (session_b, tool_use_b)] {
        let follow_up = follow_up_body_with_tool_result(
            session_id,
            tool_use_id,
            true,
            json!("error: project-filter regression fixture"),
        );
        let resp = client
            .post(format!("{proxy_url}/v1/messages"))
            .header("x-claude-code-session-id", session_id)
            .header("content-type", "application/json")
            .json(&follow_up)
            .send()
            .await?;
        assert!(resp.status().is_success());
    }

    Ok((emitting_a, emitting_b, project_a))
}

async fn assert_project_filter_isolates(
    client: &reqwest::Client,
    inspect_url: &str,
    project_a: &str,
    emitting_a: i64,
    emitting_b: i64,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!(
            "{inspect_url}/api/tool-results/errors?project={project_a}&date_from=2000-01-01T00:00:00Z"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    let rows = body["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("rows must be an array"))?;
    assert!(
        rows.iter()
            .any(|r| r["request_id"].as_i64() == Some(emitting_a)),
        "project_a filter must include project_a's error row"
    );
    assert!(
        !rows
            .iter()
            .any(|r| r["request_id"].as_i64() == Some(emitting_b)),
        "project_a filter must exclude project_b's error row (issue #214 repro inverted)"
    );
    assert!(
        rows.iter()
            .all(|r| r["project"].as_str() == Some(project_a)),
        "every returned row must carry the filtered project_path"
    );
    Ok(())
}

async fn assert_project_filter_substring_matches(
    client: &reqwest::Client,
    inspect_url: &str,
    emitting_a: i64,
    emitting_b: i64,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!(
            "{inspect_url}/api/tool-results/errors?project=pipeline-utilities-214a&date_from=2000-01-01T00:00:00Z"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    let rows = body["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("rows must be an array"))?;
    assert!(
        rows.iter()
            .any(|r| r["request_id"].as_i64() == Some(emitting_a)),
        "substring filter must still match project_a's error row"
    );
    assert!(
        !rows
            .iter()
            .any(|r| r["request_id"].as_i64() == Some(emitting_b)),
        "substring filter must not pull in project_b's row"
    );
    Ok(())
}

async fn assert_project_filter_bogus_returns_empty(
    client: &reqwest::Client,
    inspect_url: &str,
    emitting_a: i64,
    emitting_b: i64,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!(
            "{inspect_url}/api/tool-results/errors?project=no-such-project-214&date_from=2000-01-01T00:00:00Z"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    let rows = body["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("rows must be an array"))?;
    assert!(
        !rows
            .iter()
            .any(|r| r["request_id"].as_i64() == Some(emitting_a)
                || r["request_id"].as_i64() == Some(emitting_b)),
        "a non-matching project filter must return zero of our fixture rows"
    );
    Ok(())
}

#[tokio::test]
async fn list_tool_errors_project_filter_scopes_to_matching_project_only() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let (emitting_a, emitting_b, project_a) =
        seed_two_project_error_rows(&client, &proxy_url, &pool).await?;

    assert_project_filter_isolates(&client, &inspect_url, project_a, emitting_a, emitting_b)
        .await?;
    assert_project_filter_substring_matches(&client, &inspect_url, emitting_a, emitting_b).await?;
    assert_project_filter_bogus_returns_empty(&client, &inspect_url, emitting_a, emitting_b)
        .await?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
