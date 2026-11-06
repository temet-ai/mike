use crate::db_helpers;
use serde_json::Value;

#[tokio::test]
async fn list_steerers_project_filter_includes_legacy_alias_project_id() -> anyhow::Result<()> {
    let upstream = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let run_marker = uuid::Uuid::new_v4().simple().to_string();
    let canonical_path = format!("/Users/rekey-test/projects/legacy-alias-{run_marker}/repo");
    let legacy_path = format!("projects/legacy-alias-{run_marker}/repo");

    let legacy_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name) VALUES ($1, $1) RETURNING id",
    )
    .bind(&legacy_path)
    .fetch_one(&pool)
    .await?;
    sqlx::query("INSERT INTO projects.projects (project_path, display_name) VALUES ($1, $1)")
        .bind(&canonical_path)
        .execute(&pool)
        .await?;

    let steerer_name = format!("legacy-alias-steerer-{run_marker}");
    let create_resp = client
        .post(format!("{inspect_url}/api/steerers"))
        .json(&serde_json::json!({
            "name": steerer_name,
            "display_name": "Legacy Alias Steerer",
            "authority": "redirect",
            "mode": "augment",
            "model": "openrouter/meta-llama/llama-3.1-8b-instruct",
            "project_path": legacy_path,
        }))
        .send()
        .await?;
    assert_eq!(create_resp.status(), reqwest::StatusCode::CREATED);
    let create_body: Value = create_resp.json().await?;
    assert_eq!(
        create_body["steerer"]["project_id"].as_i64(),
        Some(legacy_id),
        "setup: steerer must resolve to the legacy row's id"
    );

    let list_resp = client
        .get(format!("{inspect_url}/api/steerers"))
        .query(&[("project", canonical_path.as_str())])
        .send()
        .await?;
    assert_eq!(list_resp.status(), 200);
    let steerers: Value = list_resp.json().await?;
    let names: Vec<&str> = steerers
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected an array, got {steerers}"))?
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert!(
        names.contains(&steerer_name.as_str()),
        "a steerer attached to the legacy-alias project id must still \
         appear when filtering by the canonical path: {names:?}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
