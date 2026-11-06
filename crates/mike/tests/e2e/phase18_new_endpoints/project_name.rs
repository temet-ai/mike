use crate::db_helpers;
use serde_json::Value;

use super::support::{ensure_parents, start_upstream_stub};

fn project_name_url(inspect_url: &str, segment: &str) -> anyhow::Result<reqwest::Url> {
    let mut u = reqwest::Url::parse(inspect_url)?;
    u.path_segments_mut()
        .map_err(|()| anyhow::anyhow!("inspect_url cannot be a base"))?
        .push("api")
        .push("projects")
        .push(segment)
        .push("name");
    Ok(u)
}

async fn assert_set_and_reflects_name(
    client: &reqwest::Client,
    inspect_url: &str,
    project_path: &str,
) -> anyhow::Result<()> {
    let set_resp = client
        .patch(project_name_url(inspect_url, project_path)?)
        .json(&serde_json::json!({"custom_name": "  My Friendly Project  "}))
        .send()
        .await?;
    anyhow::ensure!(
        set_resp.status().is_success(),
        "PATCH .../name (set) failed: {}",
        set_resp.status()
    );
    let set_body: Value = set_resp.json().await?;
    anyhow::ensure!(
        set_body["custom_name"].as_str() == Some("My Friendly Project"),
        "expected trimmed custom_name in PATCH response, got {set_body}"
    );

    let list_after_set: Value = client
        .get(format!("{inspect_url}/api/projects"))
        .send()
        .await?
        .json()
        .await?;
    let entry_after_set = list_after_set
        .as_array()
        .and_then(|projects| {
            projects
                .iter()
                .find(|p| p["project_path"].as_str() == Some(project_path))
        })
        .ok_or_else(|| anyhow::anyhow!("seeded project {project_path} not found after set"))?;
    anyhow::ensure!(
        entry_after_set["custom_name"].as_str() == Some("My Friendly Project"),
        "GET /api/projects must reflect the set custom_name, got {entry_after_set}"
    );
    Ok(())
}

async fn assert_clear_via_empty_string(
    client: &reqwest::Client,
    inspect_url: &str,
    project_path: &str,
) -> anyhow::Result<()> {
    let clear_via_empty = client
        .patch(project_name_url(inspect_url, project_path)?)
        .json(&serde_json::json!({"custom_name": "   "}))
        .send()
        .await?;
    anyhow::ensure!(
        clear_via_empty.status().is_success(),
        "PATCH .../name (empty-string clear) failed: {}",
        clear_via_empty.status()
    );
    let clear_via_empty_body: Value = clear_via_empty.json().await?;
    anyhow::ensure!(
        clear_via_empty_body["custom_name"].is_null(),
        "whitespace-only custom_name must normalize to null, got {clear_via_empty_body}"
    );
    Ok(())
}

async fn assert_clear_via_null_reflects_in_list(
    client: &reqwest::Client,
    inspect_url: &str,
    project_path: &str,
) -> anyhow::Result<()> {
    let set_again = client
        .patch(project_name_url(inspect_url, project_path)?)
        .json(&serde_json::json!({"custom_name": "Second Name"}))
        .send()
        .await?;
    anyhow::ensure!(
        set_again.status().is_success(),
        "PATCH .../name (re-set) failed"
    );

    let clear_via_null = client
        .patch(project_name_url(inspect_url, project_path)?)
        .json(&serde_json::json!({"custom_name": null}))
        .send()
        .await?;
    anyhow::ensure!(
        clear_via_null.status().is_success(),
        "PATCH .../name (null clear) failed: {}",
        clear_via_null.status()
    );
    let clear_via_null_body: Value = clear_via_null.json().await?;
    anyhow::ensure!(
        clear_via_null_body["custom_name"].is_null(),
        "explicit null custom_name must clear, got {clear_via_null_body}"
    );

    let list_after_clear: Value = client
        .get(format!("{inspect_url}/api/projects"))
        .send()
        .await?
        .json()
        .await?;
    let entry_after_clear = list_after_clear
        .as_array()
        .and_then(|projects| {
            projects
                .iter()
                .find(|p| p["project_path"].as_str() == Some(project_path))
        })
        .ok_or_else(|| anyhow::anyhow!("seeded project {project_path} not found after clear"))?;
    anyhow::ensure!(
        entry_after_clear["custom_name"].is_null(),
        "GET /api/projects must reflect the cleared custom_name, got {entry_after_clear}"
    );
    Ok(())
}

#[tokio::test]
async fn patch_project_name_sets_clears_and_rejects_unknown() -> anyhow::Result<()> {
    let upstream = start_upstream_stub("msg_p18e").await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping phase18_new_endpoints");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "aa000000-0000-4000-a000-000000000005";
    let project_path = "ai/tools/proxy\\phase18";
    ensure_parents(&pool, session_id, project_path).await?;

    let client = reqwest::Client::new();

    assert_set_and_reflects_name(&client, &inspect_url, project_path).await?;
    assert_clear_via_empty_string(&client, &inspect_url, project_path).await?;
    assert_clear_via_null_reflects_in_list(&client, &inspect_url, project_path).await?;

    let unknown_resp = client
        .patch(project_name_url(&inspect_url, "no-such-phase18-project")?)
        .json(&serde_json::json!({"custom_name": "irrelevant"}))
        .send()
        .await?;
    anyhow::ensure!(
        unknown_resp.status() == reqwest::StatusCode::NOT_FOUND,
        "unknown project_path should 404, got {}",
        unknown_resp.status()
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
