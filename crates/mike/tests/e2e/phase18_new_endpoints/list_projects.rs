use crate::db_helpers;
use serde_json::Value;

use super::support::{ensure_parents, start_upstream_stub};

#[tokio::test]
async fn list_projects_returns_seeded_project() -> anyhow::Result<()> {
    let upstream = start_upstream_stub("msg_p18").await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping phase18_new_endpoints");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "aa000000-0000-4000-a000-000000000001";
    let project_path = "phase18-list-projects-test";
    ensure_parents(&pool, session_id, project_path).await?;

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/projects"))
        .send()
        .await?;
    anyhow::ensure!(resp.status().is_success(), "GET /api/projects failed");
    let body: Value = resp.json().await?;

    let projects = body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected JSON array"))?;

    let found = projects
        .iter()
        .any(|p| p["project_path"].as_str() == Some(project_path));
    anyhow::ensure!(
        found,
        "seeded project {project_path} not found in /api/projects response"
    );

    for entry in projects {
        for field in [
            "project_path",
            "last_seen",
            "first_seen",
            "pinned",
            "ttl_override",
            "pin_created_at",
            "ttl_override_set_at",
        ] {
            anyhow::ensure!(
                entry.get(field).is_some(),
                "project entry missing field '{field}': {entry}"
            );
        }
    }

    for entry in projects {
        for field in ["session_count", "request_count"] {
            anyhow::ensure!(
                entry.get(field).is_none(),
                "list entry must not carry '{field}': {entry}"
            );
        }
    }

    let counted: Value = reqwest::Client::new()
        .get(format!("{inspect_url}/api/projects?counts=true"))
        .send()
        .await?
        .json()
        .await?;
    let seeded = counted
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|p| p["project_path"].as_str() == Some(project_path))
        })
        .ok_or_else(|| anyhow::anyhow!("seeded project missing from counts=true response"))?;
    anyhow::ensure!(
        seeded["session_count"].is_u64() && seeded["request_count"].is_u64(),
        "counts=true must add numeric session_count and request_count: {seeded}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_projects_reports_pin_and_ttl_override_state() -> anyhow::Result<()> {
    let upstream = start_upstream_stub("msg_p18b").await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping phase18_new_endpoints");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "aa000000-0000-4000-a000-000000000002";
    let project_path = "phase18-pin-ttl-projects-test";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    sqlx::query(
        "UPDATE projects.projects
         SET pinned = TRUE,
             pin_created_at = NOW(),
             ttl_override = INTERVAL '2 hours',
             ttl_override_set_at = NOW()
         WHERE id = $1",
    )
    .bind(project_id)
    .execute(&pool)
    .await?;

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/projects"))
        .send()
        .await?;
    anyhow::ensure!(resp.status().is_success(), "GET /api/projects failed");
    let body: Value = resp.json().await?;
    let projects = body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected JSON array"))?;

    let entry = projects
        .iter()
        .find(|p| p["project_path"].as_str() == Some(project_path))
        .ok_or_else(|| anyhow::anyhow!("seeded project {project_path} not found"))?;

    anyhow::ensure!(
        entry["pinned"].as_bool() == Some(true),
        "expected pinned=true, got {entry}"
    );
    anyhow::ensure!(
        entry["pin_created_at"].is_string(),
        "expected pin_created_at to be a non-null timestamp, got {entry}"
    );
    anyhow::ensure!(
        entry["ttl_override"].as_str() == Some("2h"),
        "expected ttl_override to round-trip as humantime \"2h\", got {entry}"
    );
    anyhow::ensure!(
        entry["ttl_override_set_at"].is_string(),
        "expected ttl_override_set_at to be a non-null timestamp, got {entry}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
