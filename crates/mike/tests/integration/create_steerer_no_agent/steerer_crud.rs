use crate::db_helpers;

use serde_json::Value;
use uuid::Uuid;

#[tokio::test]
async fn create_steerer_without_agent_id_bootstraps_agent() -> anyhow::Result<()> {
    let upstream = wiremock::MockServer::start().await;

    let Some(env) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             create_steerer_without_agent_id_bootstraps_agent"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();

    let steerer_name = format!("test-bootstrap-steerer-{}", Uuid::new_v4());
    let resp = client
        .post(format!("{}/api/steerers", env.inspect_url))
        .json(&serde_json::json!({
            "name": steerer_name,
            "display_name": "Bootstrap Test Steerer",
            "description": "Created without agent_id to test bootstrapping",
            "authority": "command",
            "mode": "drive",
            "model": "openrouter/meta-llama/llama-3.1-8b-instruct"
        }))
        .send()
        .await?;

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::CREATED,
        "expected 201 CREATED; got {} — persistence may be disabled or steerer insert failed",
        resp.status()
    );

    let body: Value = resp.json().await?;

    let agent_id = body["steerer"]["agent_id"].as_i64().ok_or_else(|| {
        anyhow::anyhow!(
            "steerer.agent_id must be a number, got {:?}",
            body["steerer"]["agent_id"]
        )
    })?;
    assert!(
        agent_id > 0,
        "expected a bootstrapped agent_id > 0, got {agent_id}"
    );

    let agent_name: String = sqlx::query_scalar("SELECT name FROM agents.agents WHERE id = $1")
        .bind(agent_id)
        .fetch_one(&env.pool)
        .await?;

    assert_eq!(
        agent_name, steerer_name,
        "bootstrapped agent name must match the steerer name slug"
    );

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}

#[tokio::test]
async fn steerer_prompt_round_trips() -> anyhow::Result<()> {
    let upstream = wiremock::MockServer::start().await;

    let Some(env) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping steerer_prompt_round_trips");
        return Ok(());
    };

    let client = reqwest::Client::new();

    let steerer_name = format!("prompt-test-steerer-{}", Uuid::new_v4());
    let create_resp = client
        .post(format!("{}/api/steerers", env.inspect_url))
        .json(&serde_json::json!({
            "name": steerer_name,
            "display_name": "Prompt Test",
            "authority": "command",
            "mode": "drive",
            "model": "google/gemini-3.5-flash-lite",
            "prompt": "You are a test steerer."
        }))
        .send()
        .await?;

    assert_eq!(
        create_resp.status(),
        reqwest::StatusCode::CREATED,
        "create with prompt should return 201"
    );
    let body: serde_json::Value = create_resp.json().await?;
    assert_eq!(
        body["steerer"]["prompt"].as_str(),
        Some("You are a test steerer."),
        "create: prompt must round-trip in the response envelope"
    );
    let steerer_id = body["steerer"]["id"].as_i64().ok_or_else(|| {
        anyhow::anyhow!(
            "steerer.id must be a number, got {:?}",
            body["steerer"]["id"]
        )
    })?;

    let get_resp = client
        .get(format!("{}/api/steerers/{steerer_id}", env.inspect_url))
        .send()
        .await?;
    assert_eq!(get_resp.status(), reqwest::StatusCode::OK);
    let get_body: serde_json::Value = get_resp.json().await?;
    assert_eq!(
        get_body["prompt"].as_str(),
        Some("You are a test steerer."),
        "GET: prompt must be returned from DB"
    );

    let patch_resp = client
        .patch(format!("{}/api/steerers/{steerer_id}", env.inspect_url))
        .json(&serde_json::json!({ "prompt": "Updated prompt." }))
        .send()
        .await?;
    assert_eq!(patch_resp.status(), reqwest::StatusCode::OK);
    let patch_body: serde_json::Value = patch_resp.json().await?;
    assert_eq!(
        patch_body["steerer"]["prompt"].as_str(),
        Some("Updated prompt."),
        "PATCH: updated prompt must appear in response"
    );

    let db_prompt: Option<String> =
        sqlx::query_scalar("SELECT prompt FROM steering.steerers WHERE id = $1")
            .bind(steerer_id)
            .fetch_one(&env.pool)
            .await?;
    assert_eq!(
        db_prompt.as_deref(),
        Some("Updated prompt."),
        "DB: prompt column must reflect the PATCH value"
    );

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}
