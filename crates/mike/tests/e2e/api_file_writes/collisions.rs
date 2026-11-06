use crate::db_helpers;

use serde_json::Value;

use super::fixtures::{ToolCallSeed, insert_tool_call};

struct CollisionFixture<'a> {
    project: &'a str,
    session_a: &'a str,
    session_b: &'a str,
    shared: &'a str,
    solo: &'a str,
    churned: &'a str,
}

async fn seed_collision_fixture(
    pool: &sqlx::PgPool,
    f: &CollisionFixture<'_>,
) -> anyhow::Result<()> {
    insert_tool_call(
        pool,
        ToolCallSeed {
            project_path: f.project,
            session_id: f.session_b,
            tool_name: "Edit",
            operation: "write",
            file_path: f.shared,
            wire_id: "toolu_fw_b1",
            offset_ms: 0,
        },
    )
    .await?;
    insert_tool_call(
        pool,
        ToolCallSeed {
            project_path: f.project,
            session_id: f.session_a,
            tool_name: "Edit",
            operation: "write",
            file_path: f.shared,
            wire_id: "toolu_fw_a1",
            offset_ms: 500,
        },
    )
    .await?;
    insert_tool_call(
        pool,
        ToolCallSeed {
            project_path: f.project,
            session_id: f.session_a,
            tool_name: "Write",
            operation: "write",
            file_path: f.solo,
            wire_id: "toolu_fw_a2",
            offset_ms: 100,
        },
    )
    .await?;
    insert_tool_call(
        pool,
        ToolCallSeed {
            project_path: f.project,
            session_id: f.session_a,
            tool_name: "Edit",
            operation: "write",
            file_path: f.churned,
            wire_id: "toolu_fw_a3",
            offset_ms: 200,
        },
    )
    .await?;
    insert_tool_call(
        pool,
        ToolCallSeed {
            project_path: f.project,
            session_id: f.session_a,
            tool_name: "Edit",
            operation: "write",
            file_path: f.churned,
            wire_id: "toolu_fw_a4",
            offset_ms: 300,
        },
    )
    .await?;
    insert_tool_call(
        pool,
        ToolCallSeed {
            project_path: f.project,
            session_id: f.session_b,
            tool_name: "Read",
            operation: "read",
            file_path: f.shared,
            wire_id: "toolu_fw_b2",
            offset_ms: 400,
        },
    )
    .await?;
    Ok(())
}

async fn assert_collisions_only_response(
    client: &reqwest::Client,
    inspect_url: &str,
    project: &str,
    session_a: &str,
    session_b: &str,
    shared: &str,
) -> anyhow::Result<()> {
    let body: Value = client
        .get(format!(
            "{inspect_url}/api/file-writes?project={project}&collisions_only=true"
        ))
        .send()
        .await?
        .json()
        .await?;

    let files = body["files"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("files must be an array"))?;
    assert_eq!(
        files.len(),
        1,
        "only the two-session file is a collision; solo and churned are not: {body}"
    );
    assert_eq!(body["total_count"].as_i64(), Some(1));

    let hit = &files[0];
    assert_eq!(hit["file_path"].as_str(), Some(shared));
    assert_eq!(hit["session_count"].as_i64(), Some(2));
    assert_eq!(
        hit["write_count"].as_i64(),
        Some(2),
        "the Read of this file must not count as a write: {hit}"
    );

    let writes = hit["writes"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("writes must be an array"))?;
    assert_eq!(writes.len(), 2);
    assert_eq!(
        writes[0]["session_id"].as_str(),
        Some(session_b),
        "B wrote first, so B is first: {hit}"
    );
    assert_eq!(writes[1]["session_id"].as_str(), Some(session_a));

    assert!(
        body["coverage"]
            .as_str()
            .is_some_and(|c| c.contains("Bash")),
        "the response must state what it cannot see: {body}"
    );
    Ok(())
}

async fn assert_unfiltered_response(
    client: &reqwest::Client,
    inspect_url: &str,
    project: &str,
) -> anyhow::Result<()> {
    let all: Value = client
        .get(format!("{inspect_url}/api/file-writes?project={project}"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        all["total_count"].as_i64(),
        Some(3),
        "shared + solo + churned: {all}"
    );
    Ok(())
}

#[tokio::test]
async fn file_writes_surfaces_cross_session_collisions_in_order() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let fixture = CollisionFixture {
        project: "collision-fixture-project",
        session_a: "b0000000-0000-4000-8000-000000000a01",
        session_b: "b0000000-0000-4000-8000-000000000b01",
        shared: "/repo/src/shared.rs",
        solo: "/repo/src/solo.rs",
        churned: "/repo/src/churned.rs",
    };

    seed_collision_fixture(&pool, &fixture).await?;

    let client = reqwest::Client::new();
    assert_collisions_only_response(
        &client,
        &inspect_url,
        fixture.project,
        fixture.session_a,
        fixture.session_b,
        fixture.shared,
    )
    .await?;
    assert_unfiltered_response(&client, &inspect_url, fixture.project).await?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn file_writes_window_bounds_and_rejects_a_bad_bound() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let project = "window-fixture-project";
    let session = "b0000000-0000-4000-8000-000000000c01";
    insert_tool_call(
        &pool,
        ToolCallSeed {
            project_path: project,
            session_id: session,
            tool_name: "Edit",
            operation: "write",
            file_path: "/repo/src/old.rs",
            wire_id: "toolu_fw_old",
            offset_ms: -600_000,
        },
    )
    .await?;
    insert_tool_call(
        &pool,
        ToolCallSeed {
            project_path: project,
            session_id: session,
            tool_name: "Edit",
            operation: "write",
            file_path: "/repo/src/new.rs",
            wire_id: "toolu_fw_new",
            offset_ms: 0,
        },
    )
    .await?;

    let client = reqwest::Client::new();
    let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(1))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let windowed: Value = client
        .get(format!(
            "{inspect_url}/api/file-writes?project={project}&from={cutoff}"
        ))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        windowed["total_count"].as_i64(),
        Some(1),
        "only the recent write is inside the window: {windowed}"
    );
    assert_eq!(
        windowed["files"][0]["file_path"].as_str(),
        Some("/repo/src/new.rs")
    );

    let plus_offset = (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
    let recovered: Value = client
        .get(format!(
            "{inspect_url}/api/file-writes?project={project}&from={plus_offset}"
        ))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        recovered["total_count"].as_i64(),
        Some(1),
        "an unencoded +00:00 offset must resolve to the same window: {recovered}"
    );

    let bad = client
        .get(format!(
            "{inspect_url}/api/file-writes?project={project}&from=last-tuesday"
        ))
        .send()
        .await?;
    assert_eq!(
        bad.status(),
        400,
        "an unparseable `from` is a 400, never a silently unbounded window"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
