use crate::db_helpers;
use db_helpers::ProxyWithDb;

use serde_json::Value;

async fn assert_scoped_create_resolves_project(
    client: &reqwest::Client,
    env: &ProxyWithDb,
    project_path: &str,
    project_id: i64,
    run_id: &str,
) -> anyhow::Result<i64> {
    let scoped_resp = client
        .post(format!("{}/api/steerers", env.inspect_url))
        .json(&serde_json::json!({
            "name": format!("scope-test-scoped-{run_id}"),
            "display_name": "Scope Test (scoped)",
            "authority": "command",
            "mode": "drive",
            "model": "openrouter/meta-llama/llama-3.1-8b-instruct",
            "project_path": project_path,
        }))
        .send()
        .await?;
    assert_eq!(
        scoped_resp.status(),
        reqwest::StatusCode::CREATED,
        "scoped create should return 201"
    );
    let scoped_body: Value = scoped_resp.json().await?;
    assert_eq!(
        scoped_body["steerer"]["project_id"].as_i64(),
        Some(project_id),
        "scoped create must return the resolved project_id; got {scoped_body}"
    );
    assert_eq!(
        scoped_body["steerer"]["project_path"].as_str(),
        Some(project_path),
        "scoped create must return the joined project_path; got {scoped_body}"
    );
    scoped_body["steerer"]["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("scoped steerer.id must be a number"))
}

async fn assert_unknown_project_path_rejected(
    client: &reqwest::Client,
    env: &ProxyWithDb,
    run_id: &str,
) -> anyhow::Result<()> {
    let unknown_resp = client
        .post(format!("{}/api/steerers", env.inspect_url))
        .json(&serde_json::json!({
            "name": format!("scope-test-unknown-project-{run_id}"),
            "display_name": "Scope Test (unknown project)",
            "authority": "command",
            "mode": "drive",
            "model": "openrouter/meta-llama/llama-3.1-8b-instruct",
            "project_path": "no-such-project-scope-round-trip",
        }))
        .send()
        .await?;
    assert_eq!(
        unknown_resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "an unknown project_path must 400, not silently create a global steerer"
    );
    Ok(())
}

async fn assert_global_create_leaves_scope_null(
    client: &reqwest::Client,
    env: &ProxyWithDb,
    run_id: &str,
) -> anyhow::Result<i64> {
    let global_resp = client
        .post(format!("{}/api/steerers", env.inspect_url))
        .json(&serde_json::json!({
            "name": format!("scope-test-global-{run_id}"),
            "display_name": "Scope Test (global)",
            "authority": "command",
            "mode": "drive",
            "model": "openrouter/meta-llama/llama-3.1-8b-instruct",
        }))
        .send()
        .await?;
    assert_eq!(global_resp.status(), reqwest::StatusCode::CREATED);
    let global_body: Value = global_resp.json().await?;
    assert!(
        global_body["steerer"]["project_id"].is_null(),
        "steerer created without project_path must be global; got {global_body}"
    );
    assert!(
        global_body["steerer"]["project_path"].is_null(),
        "steerer created without project_path must be global; got {global_body}"
    );
    global_body["steerer"]["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("global steerer.id must be a number"))
}

async fn assert_list_reflects_both_scopes(
    client: &reqwest::Client,
    env: &ProxyWithDb,
    scoped_id: i64,
    project_id: i64,
    project_path: &str,
    global_id: i64,
) -> anyhow::Result<()> {
    let list_resp = client
        .get(format!("{}/api/steerers", env.inspect_url))
        .send()
        .await?;
    assert_eq!(list_resp.status(), reqwest::StatusCode::OK);
    let list_body: Value = list_resp.json().await?;
    let rows = list_body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("GET /api/steerers must return a JSON array"))?;
    let listed_scoped = rows
        .iter()
        .find(|r| r["id"].as_i64() == Some(scoped_id))
        .ok_or_else(|| anyhow::anyhow!("scoped steerer missing from list; got {list_body}"))?;
    assert_eq!(listed_scoped["project_id"].as_i64(), Some(project_id));
    assert_eq!(listed_scoped["project_path"].as_str(), Some(project_path));
    let listed_global = rows
        .iter()
        .find(|r| r["id"].as_i64() == Some(global_id))
        .ok_or_else(|| anyhow::anyhow!("global steerer missing from list; got {list_body}"))?;
    assert!(listed_global["project_id"].is_null());
    assert!(listed_global["project_path"].is_null());
    Ok(())
}

async fn assert_patch_double_option_semantics(
    client: &reqwest::Client,
    env: &ProxyWithDb,
    scoped_id: i64,
    project_id: i64,
    project_path: &str,
) -> anyhow::Result<()> {
    let patch_noop_resp = client
        .patch(format!("{}/api/steerers/{scoped_id}", env.inspect_url))
        .json(&serde_json::json!({ "priority": 5 }))
        .send()
        .await?;
    assert_eq!(patch_noop_resp.status(), reqwest::StatusCode::OK);
    let patch_noop_body: Value = patch_noop_resp.json().await?;
    assert_eq!(
        patch_noop_body["steerer"]["project_id"].as_i64(),
        Some(project_id),
        "PATCH omitting project_path must leave scope unchanged; got {patch_noop_body}"
    );

    let patch_clear_resp = client
        .patch(format!("{}/api/steerers/{scoped_id}", env.inspect_url))
        .json(&serde_json::json!({ "project_path": null }))
        .send()
        .await?;
    assert_eq!(patch_clear_resp.status(), reqwest::StatusCode::OK);
    let patch_clear_body: Value = patch_clear_resp.json().await?;
    assert!(
        patch_clear_body["steerer"]["project_id"].is_null(),
        "PATCH with explicit null project_path must clear to global; got {patch_clear_body}"
    );

    let patch_set_resp = client
        .patch(format!("{}/api/steerers/{scoped_id}", env.inspect_url))
        .json(&serde_json::json!({ "project_path": project_path }))
        .send()
        .await?;
    assert_eq!(patch_set_resp.status(), reqwest::StatusCode::OK);
    let patch_set_body: Value = patch_set_resp.json().await?;
    assert_eq!(
        patch_set_body["steerer"]["project_id"].as_i64(),
        Some(project_id),
        "PATCH with a project_path string must re-scope; got {patch_set_body}"
    );

    Ok(())
}

#[tokio::test]
async fn create_steerer_project_scope_round_trips() -> anyhow::Result<()> {
    let upstream = wiremock::MockServer::start().await;

    let Some(env) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping create_steerer_project_scope_round_trips");
        return Ok(());
    };

    let client = reqwest::Client::new();

    let run_id = uuid::Uuid::new_v4().simple().to_string();
    let project_path = format!("scope-round-trip-project-{run_id}");
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name) VALUES ($1, $1) RETURNING id",
    )
    .bind(&project_path)
    .fetch_one(&env.pool)
    .await?;

    let scoped_id =
        assert_scoped_create_resolves_project(&client, &env, &project_path, project_id, &run_id)
            .await?;
    assert_unknown_project_path_rejected(&client, &env, &run_id).await?;
    let global_id = assert_global_create_leaves_scope_null(&client, &env, &run_id).await?;
    assert_list_reflects_both_scopes(
        &client,
        &env,
        scoped_id,
        project_id,
        &project_path,
        global_id,
    )
    .await?;
    assert_patch_double_option_semantics(&client, &env, scoped_id, project_id, &project_path)
        .await?;

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}
