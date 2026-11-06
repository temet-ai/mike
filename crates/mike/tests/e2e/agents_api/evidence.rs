use crate::db_helpers;
use crate::seed;

use chrono::Utc;
use seed::{SeedRequest, seed_project};
use serde_json::Value;
use sqlx::PgPool;

use super::fixtures::{
    SessionPromptSeed, seed_agent, seed_agent_tool_use, seed_alias, seed_dispatch_for_agent,
    seed_instance, seed_session_prompt,
};

#[tokio::test]
async fn list_agents_reports_evidence_counts() -> anyhow::Result<()> {
    let Some(proxy) = db_helpers::start_proxy_with_shared_db_no_local_fallback(
        "http://upstream-not-used-by-this-test.invalid",
    )
    .await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let agent_id = seed_agent(&pool, "evidence-agent").await?;
    let project_id = seed_project(&pool, "agents-api/evidence-list").await?;
    let session_id = uuid::Uuid::new_v4().to_string();

    let t0 = Utc::now();
    SeedRequest::new(910_001, project_id)
        .session_uuid_ensure(&session_id)
        .created_at(t0)
        .insert(&pool)
        .await?;
    let tool_use_id = seed_agent_tool_use(
        &pool,
        "tu-evidence-1",
        910_001,
        0,
        t0,
        "do the evidence thing",
    )
    .await?;
    let dispatch_id = seed_dispatch_for_agent(&pool, tool_use_id, agent_id).await?;
    let session_pk: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await?;
    seed_instance(
        &pool,
        session_pk,
        dispatch_id,
        agent_id,
        Some("evidence-persona"),
    )
    .await?;
    seed_session_prompt(
        &pool,
        SessionPromptSeed {
            session_pk,
            request_id: 910_001,
            agent_id,
            lane: "main",
            content_sha256: &"e".repeat(64),
            created_at: t0,
        },
    )
    .await?;
    seed_alias(&pool, agent_id, "evidence-persona", "persona").await?;
    seed_alias(&pool, agent_id, "evidence-agent-alt", "name_variant").await?;

    let resp = client
        .get(format!("{inspect_url}/api/agents"))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Vec<Value> = resp.json().await?;
    let row = body
        .iter()
        .find(|r| r["id"].as_i64() == Some(agent_id))
        .ok_or_else(|| anyhow::anyhow!("seeded agent missing from list"))?;

    assert_eq!(row["composition_count"].as_i64(), Some(0));
    assert_eq!(row["dispatch_count"].as_i64(), Some(1));
    assert_eq!(row["instance_count"].as_i64(), Some(1));
    assert_eq!(row["prompt_version_count"].as_i64(), Some(1));
    assert!(
        row["last_dispatched_at"].is_string(),
        "last_dispatched_at must be a timestamp once a dispatch exists"
    );
    let aliases = row["aliases"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("aliases must be an array"))?;
    assert_eq!(aliases.len(), 2);
    assert!(aliases.iter().any(|alias| {
        alias["alias"].as_str() == Some("evidence-persona")
            && alias["kind"].as_str() == Some("persona")
    }));
    assert!(aliases.iter().any(|alias| {
        alias["alias"].as_str() == Some("evidence-agent-alt")
            && alias["kind"].as_str() == Some("name_variant")
    }));

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

struct DetailFixture {
    agent_id: i64,
    session_id: String,
    full_prompt: &'static str,
}

async fn seed_detail_fixture(pool: &PgPool) -> anyhow::Result<DetailFixture> {
    let agent_id = seed_agent(pool, "detail-evidence-agent").await?;
    let project_id = seed_project(pool, "agents-api/evidence-detail").await?;
    let session_id = uuid::Uuid::new_v4().to_string();

    let t0 = Utc::now();
    let t1 = t0 + chrono::Duration::seconds(1);
    SeedRequest::new(920_001, project_id)
        .session_uuid_ensure(&session_id)
        .created_at(t0)
        .insert(pool)
        .await?;
    SeedRequest::new(920_002, project_id)
        .session_uuid(&session_id)
        .created_at(t1)
        .insert(pool)
        .await?;
    let session_pk: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&session_id)
            .fetch_one(pool)
            .await?;

    let full_prompt = "investigate the full detail-evidence prompt text end to end";
    let tu_a = seed_agent_tool_use(pool, "tu-detail-a", 920_001, 0, t0, full_prompt).await?;
    let tu_b = seed_agent_tool_use(
        pool,
        "tu-detail-b",
        920_002,
        0,
        t1,
        "second dispatch prompt",
    )
    .await?;
    let dispatch_a = seed_dispatch_for_agent(pool, tu_a, agent_id).await?;
    seed_dispatch_for_agent(pool, tu_b, agent_id).await?;
    seed_instance(
        pool,
        session_pk,
        dispatch_a,
        agent_id,
        Some("detail-evidence-persona"),
    )
    .await?;
    seed_session_prompt(
        pool,
        SessionPromptSeed {
            session_pk,
            request_id: 920_001,
            agent_id,
            lane: "main",
            content_sha256: &"f".repeat(64),
            created_at: t0,
        },
    )
    .await?;

    Ok(DetailFixture {
        agent_id,
        session_id,
        full_prompt,
    })
}

async fn fetch_agent_detail(
    client: &reqwest::Client,
    inspect_url: &str,
    agent_id: i64,
) -> anyhow::Result<Value> {
    let resp = client
        .get(format!("{inspect_url}/api/agents/{agent_id}"))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    Ok(resp.json().await?)
}

fn assert_full_detail_page(body: &Value, fixture: &DetailFixture) -> anyhow::Result<()> {
    assert_eq!(body["dispatches"]["total_count"].as_i64(), Some(2));
    let rows = body["dispatches"]["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("dispatches.rows must be an array"))?;
    assert_eq!(rows.len(), 2, "default page must return both dispatches");
    assert_eq!(
        rows[0]["prompt"].as_str(),
        Some("second dispatch prompt"),
        "newest dispatch (t1) must sort first"
    );
    assert_eq!(rows[1]["prompt"].as_str(), Some(fixture.full_prompt));
    assert_eq!(
        rows[1]["session_uuid"].as_str(),
        Some(fixture.session_id.as_str())
    );

    let prompt_versions = body["prompt_versions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("prompt_versions must be an array"))?;
    assert_eq!(prompt_versions.len(), 1);
    assert_eq!(
        prompt_versions[0]["content_sha256"].as_str(),
        Some("f".repeat(64).as_str())
    );
    assert_eq!(prompt_versions[0]["request_count"].as_i64(), Some(1));

    let instances = body["instances"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("instances must be an array"))?;
    assert_eq!(instances.len(), 1);
    assert_eq!(
        instances[0]["subagent_persona"].as_str(),
        Some("detail-evidence-persona")
    );
    assert_eq!(
        instances[0]["session_uuid"].as_str(),
        Some(fixture.session_id.as_str())
    );
    Ok(())
}

async fn assert_paginated_detail_page(
    client: &reqwest::Client,
    inspect_url: &str,
    agent_id: i64,
) -> anyhow::Result<()> {
    let paged = client
        .get(format!(
            "{inspect_url}/api/agents/{agent_id}?dispatch_limit=1&dispatch_offset=0"
        ))
        .send()
        .await?;
    assert_eq!(paged.status(), 200);
    let paged_body: Value = paged.json().await?;
    assert_eq!(paged_body["dispatches"]["total_count"].as_i64(), Some(2));
    let paged_rows = paged_body["dispatches"]["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("paged dispatches.rows must be an array"))?;
    assert_eq!(paged_rows.len(), 1, "dispatch_limit=1 must return one row");
    assert_eq!(
        paged_rows[0]["prompt"].as_str(),
        Some("second dispatch prompt")
    );
    Ok(())
}

#[tokio::test]
async fn get_agent_detail_returns_dispatches_prompt_versions_and_instances() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_shared_db("http://upstream-not-used-by-this-test.invalid")
            .await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let fixture = seed_detail_fixture(&pool).await?;

    let body = fetch_agent_detail(&client, &inspect_url, fixture.agent_id).await?;
    assert_full_detail_page(&body, &fixture)?;
    assert_paginated_detail_page(&client, &inspect_url, fixture.agent_id).await?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
