use crate::seed::{seed_project, seed_session};
use crate::test_db::TestDb;

use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use super::support::{boot_proxy, ok_response};

#[tokio::test]
async fn thread_participant_identities_resolve_title_role_and_short_uuid_fallback()
-> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping thread_participant_identities_resolve_title_role_and_short_uuid_fallback"
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

    let project_id = seed_project(&pool, "participant-identities-test").await?;
    let (alice, bob, carol) = seed_identity_tier_sessions(&pool, project_id).await?;

    let (thread_uuid, detail) =
        create_thread_and_fetch_detail(&client, &inspect_url, &alice, &bob, &carol).await?;
    assert_identity_precedence(&detail, &alice, &bob, &carol)?;
    assert_list_route_shares_identity_resolution(&client, &inspect_url, &alice, &thread_uuid)
        .await?;

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

async fn seed_identity_tier_sessions(
    pool: &PgPool,
    project_id: i64,
) -> anyhow::Result<(String, String, String)> {
    let alice = uuid::Uuid::new_v4().to_string();
    seed_session(pool, project_id, &alice).await?;
    sqlx::query("UPDATE sessions.sessions SET title = $1 WHERE session_uuid = $2::text::uuid")
        .bind("Rollout coordinator")
        .bind(&alice)
        .execute(pool)
        .await?;

    let bob = uuid::Uuid::new_v4().to_string();
    seed_session(pool, project_id, &bob).await?;
    let bob_db_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&bob)
            .fetch_one(pool)
            .await?;
    sqlx::query(
        "INSERT INTO sessions.session_goals (session_id, goal, role) VALUES ($1, 'ship the rollout', $2)",
    )
    .bind(bob_db_id)
    .bind("rust-developer")
    .execute(pool)
    .await?;

    let carol = uuid::Uuid::new_v4().to_string();
    seed_session(pool, project_id, &carol).await?;

    Ok((alice, bob, carol))
}

async fn create_thread_and_fetch_detail(
    client: &reqwest::Client,
    inspect_url: &str,
    alice: &str,
    bob: &str,
    carol: &str,
) -> anyhow::Result<(String, Value)> {
    let create_resp = client
        .post(format!("{inspect_url}/api/threads"))
        .json(&json!({
            "subject": "identity resolution",
            "body": "kickoff",
            "from_session_id": alice,
            "notify": [bob, carol],
        }))
        .send()
        .await?;
    anyhow::ensure!(
        create_resp.status() == 201,
        "create returned {}: {}",
        create_resp.status(),
        create_resp.text().await.unwrap_or_default()
    );
    let created: Value = create_resp.json().await?;
    let thread_uuid = created["thread_uuid"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing thread_uuid: {created}"))?
        .to_string();

    let detail: Value = client
        .get(format!("{inspect_url}/api/threads/{thread_uuid}"))
        .send()
        .await?
        .json()
        .await?;
    Ok((thread_uuid, detail))
}

fn assert_identity_precedence(
    detail: &Value,
    alice: &str,
    bob: &str,
    carol: &str,
) -> anyhow::Result<()> {
    let carol_short = &carol[..8];
    let identities = detail["thread"]["participant_identities"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("participant_identities not an array: {detail}"))?;
    anyhow::ensure!(
        identities.len() == 3,
        "every participant resolves an identity entry: {detail}"
    );
    anyhow::ensure!(
        !identities
            .iter()
            .any(|i| i["label"].as_str() == Some("unknown")),
        "a participant is never labelled the literal string \"unknown\": {detail}"
    );

    let find = |uuid: &str| -> anyhow::Result<&Value> {
        identities
            .iter()
            .find(|i| i["session_uuid"].as_str() == Some(uuid))
            .ok_or_else(|| anyhow::anyhow!("no identity entry for {uuid}: {detail}"))
    };

    let alice_identity = find(alice)?;
    anyhow::ensure!(
        alice_identity["label"].as_str() == Some("Rollout coordinator"),
        "title wins over role/agent_type/short-uuid: {alice_identity}"
    );

    let bob_identity = find(bob)?;
    anyhow::ensure!(
        bob_identity["label"].as_str() == Some("rust-developer"),
        "no title: role is the next tier: {bob_identity}"
    );
    anyhow::ensure!(
        bob_identity["role"].as_str() == Some("rust-developer"),
        "role is also carried on its own field, not just folded into label: {bob_identity}"
    );

    let carol_identity = find(carol)?;
    anyhow::ensure!(
        carol_identity["label"].as_str() == Some(carol_short),
        "no title, role, or agent_type: falls back to the uuid's own first 8 hex \
         characters, never \"unknown\": {carol_identity}"
    );
    anyhow::ensure!(
        carol_identity["role"].is_null() && carol_identity["agent_type"].is_null(),
        "carol has no declared role or dispatched agent_type: {carol_identity}"
    );
    Ok(())
}

async fn assert_list_route_shares_identity_resolution(
    client: &reqwest::Client,
    inspect_url: &str,
    alice: &str,
    thread_uuid: &str,
) -> anyhow::Result<()> {
    let list: Value = client
        .get(format!("{inspect_url}/api/threads?participant={alice}"))
        .send()
        .await?
        .json()
        .await?;
    let list_thread = list["threads"]
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|t| t["thread_uuid"].as_str() == Some(thread_uuid))
        })
        .ok_or_else(|| anyhow::anyhow!("thread missing from list: {list}"))?;
    let list_identities = list_thread["participant_identities"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("participant_identities not an array: {list_thread}"))?;
    anyhow::ensure!(
        list_identities
            .iter()
            .any(|i| i["session_uuid"].as_str() == Some(alice)
                && i["label"].as_str() == Some("Rollout coordinator")),
        "GET /api/threads resolves the same identities as GET /api/threads/{{uuid}}: {list_thread}"
    );
    Ok(())
}
