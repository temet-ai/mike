use crate::seed::{SeedRequest, seed_project, seed_session};
use crate::test_db::TestDb;

use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use super::support::{ToolUseSeed, boot_proxy, ok_response, seed_tool_use};

#[tokio::test]
async fn team_comms_reflects_dispatches_and_messages() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping team_comms_reflects_dispatches_and_messages"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&db.database_url)
        .await?;
    let (_state, _proxy_url, inspect_url, shutdown) =
        boot_proxy(&mock.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();

    let project_id = seed_project(&pool, "team-comms-test").await?;
    let coordinator = uuid::Uuid::new_v4().to_string();
    seed_session(&pool, project_id, &coordinator).await?;
    let coordinator_db_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&coordinator)
            .fetch_one(&pool)
            .await?;

    seed_team_comms_traffic(&pool, project_id, coordinator_db_id).await?;
    assert_team_comms_dispatches_and_messages(&client, &inspect_url, &coordinator).await?;

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

async fn seed_team_comms_traffic(
    pool: &PgPool,
    project_id: i64,
    coordinator_db_id: i64,
) -> anyhow::Result<()> {
    let t0 = Utc::now() - Duration::minutes(10);

    let primary_req = SeedRequest::new(9_800_001, project_id)
        .session_pk(coordinator_db_id)
        .created_at(t0)
        .insert(pool)
        .await?;
    seed_tool_use(
        pool,
        ToolUseSeed {
            wire_id: "tu-sent-1",
            request_id: primary_req,
            ord: 1,
            session_db_id: coordinator_db_id,
            tool_name: "SendMessage",
            input: json!({"to": "worker-alpha", "message": "start on the auth module", "summary": "kickoff"}),
            created_at: t0,
        },
    )
    .await?;

    let sub_req_1 = SeedRequest::new(9_800_002, project_id)
        .session_pk(coordinator_db_id)
        .turn_origin("subagent")
        .subagent_persona(Some("worker-alpha"))
        .created_at(t0 + Duration::minutes(1))
        .insert(pool)
        .await?;
    seed_tool_use(
        pool,
        ToolUseSeed {
            wire_id: "tu-bash-1",
            request_id: sub_req_1,
            ord: 1,
            session_db_id: coordinator_db_id,
            tool_name: "Bash",
            input: json!({"command": "cargo check"}),
            created_at: t0 + Duration::minutes(1),
        },
    )
    .await?;

    let sub_req_2 = SeedRequest::new(9_800_003, project_id)
        .session_pk(coordinator_db_id)
        .turn_origin("subagent")
        .subagent_persona(Some("worker-alpha"))
        .created_at(t0 + Duration::minutes(2))
        .insert(pool)
        .await?;
    seed_tool_use(
        pool,
        ToolUseSeed {
            wire_id: "tu-agent-1",
            request_id: sub_req_2,
            ord: 1,
            session_db_id: coordinator_db_id,
            tool_name: "Agent",
            input: json!({"description": "explore the auth module"}),
            created_at: t0 + Duration::minutes(2),
        },
    )
    .await?;
    seed_tool_use(
        pool,
        ToolUseSeed {
            wire_id: "tu-received-1",
            request_id: sub_req_2,
            ord: 2,
            session_db_id: coordinator_db_id,
            tool_name: "SendMessage",
            input: json!({"to": "main", "message": "auth module uses argon2", "summary": null}),
            created_at: t0 + Duration::minutes(3),
        },
    )
    .await?;

    seed_tool_use(
        pool,
        ToolUseSeed {
            wire_id: "tu-peer-1",
            request_id: sub_req_2,
            ord: 3,
            session_db_id: coordinator_db_id,
            tool_name: "SendMessage",
            input: json!({"to": "worker-beta", "message": "should never surface here"}),
            created_at: t0 + Duration::minutes(4),
        },
    )
    .await?;

    Ok(())
}

async fn assert_team_comms_dispatches_and_messages(
    client: &reqwest::Client,
    inspect_url: &str,
    coordinator: &str,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{coordinator}/team-comms"
        ))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 200,
        "team-comms returned {}",
        resp.status()
    );
    let body: Value = resp.json().await?;

    let dispatches = body["dispatches"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("dispatches not an array: {body}"))?;
    anyhow::ensure!(
        dispatches.len() == 1,
        "exactly one agent label (worker-alpha) dispatched its own traffic: {body}"
    );
    let dispatch = &dispatches[0];
    anyhow::ensure!(dispatch["agent_label"].as_str() == Some("worker-alpha"));
    anyhow::ensure!(
        dispatch["description"].as_str() == Some("explore the auth module"),
        "description comes from the Agent tool_use in the same label's traffic: {dispatch}"
    );
    anyhow::ensure!(
        dispatch["request_count"].as_i64() == Some(2),
        "request_count is COUNT DISTINCT over the label's two requests: {dispatch}"
    );

    let messages = body["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages not an array: {body}"))?;
    anyhow::ensure!(
        messages.len() == 2,
        "exactly the primary sent + the subagent-to-main received rows — the peer-to-peer \
         send must be excluded: {body}"
    );
    anyhow::ensure!(
        messages[0]["ts"].as_str().unwrap_or_default()
            <= messages[1]["ts"].as_str().unwrap_or_default(),
        "messages must be oldest-first: {body}"
    );
    let sent = messages
        .iter()
        .find(|m| m["direction"].as_str() == Some("sent"))
        .ok_or_else(|| anyhow::anyhow!("no sent-direction row: {body}"))?;
    anyhow::ensure!(sent["counterpart"].as_str() == Some("worker-alpha"));
    anyhow::ensure!(sent["body"].as_str() == Some("start on the auth module"));
    anyhow::ensure!(sent["summary"].as_str() == Some("kickoff"));
    let received = messages
        .iter()
        .find(|m| m["direction"].as_str() == Some("received"))
        .ok_or_else(|| anyhow::anyhow!("no received-direction row: {body}"))?;
    anyhow::ensure!(received["counterpart"].as_str() == Some("worker-alpha"));
    anyhow::ensure!(received["body"].as_str() == Some("auth module uses argon2"));
    anyhow::ensure!(
        !messages
            .iter()
            .any(|m| m["body"].as_str() == Some("should never surface here")),
        "the peer-to-peer send (to worker-beta, not main) must never appear: {body}"
    );

    let unknown = uuid::Uuid::new_v4();
    let unknown_resp = client
        .get(format!("{inspect_url}/api/sessions/{unknown}/team-comms"))
        .send()
        .await?;
    anyhow::ensure!(
        unknown_resp.status() == 404,
        "an unknown session must 404, got {}",
        unknown_resp.status()
    );

    Ok(())
}
