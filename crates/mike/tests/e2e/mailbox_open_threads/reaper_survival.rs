use crate::seed::{seed_project, seed_session};
use crate::test_db::TestDb;

use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use super::support::{
    backdate_queued_at, boot_proxy, mailbox_row_state, ok_response, send_primary_turn,
};

#[tokio::test]
async fn null_recipient_rows_survive_claim_and_reaper() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping null_recipient_rows_survive_claim_and_reaper"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&db.database_url)
        .await?;
    let (state, proxy_url, inspect_url, shutdown) =
        boot_proxy(&mock.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();

    let project_id = seed_project(&pool, "open-threads-reaper-test").await?;
    let creator = uuid::Uuid::new_v4().to_string();
    let unrelated = uuid::Uuid::new_v4().to_string();
    seed_session(&pool, project_id, &creator).await?;
    seed_session(&pool, project_id, &unrelated).await?;

    let create_resp = client
        .post(format!("{inspect_url}/api/threads"))
        .json(&json!({
            "subject": "orphaned by design",
            "body": "nobody claims this",
            "from_session_id": creator,
            "notify": [],
        }))
        .send()
        .await?;
    anyhow::ensure!(create_resp.status() == 201);
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
    let message_uuid = uuid::Uuid::parse_str(
        detail["messages"][0]["message_uuid"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing message_uuid: {detail}"))?,
    )?;

    send_primary_turn(&client, &proxy_url, &unrelated).await?;
    let (claimed, sent, dead_lettered) = mailbox_row_state(&pool, message_uuid).await?;
    anyhow::ensure!(
        !claimed && !sent && !dead_lettered,
        "an unrelated session's primary turn must never claim a recipient-less row"
    );

    backdate_queued_at(&pool, message_uuid).await?;
    mike::mailbox_reaper::spawn_mailbox_reaper(&state);

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let (_, _, dead_lettered) = mailbox_row_state(&pool, message_uuid).await?;
        if tokio::time::Instant::now() >= deadline {
            anyhow::ensure!(
                !dead_lettered,
                "a recipient-less row must never be dead-lettered by the TTL reaper \
                 (to_session_id IS NOT NULL guards dead_letter_expired_undelivered)"
            );
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let (claimed, sent, dead_lettered) = mailbox_row_state(&pool, message_uuid).await?;
    anyhow::ensure!(
        !claimed && !sent && !dead_lettered,
        "a recipient-less row must remain completely untouched after a reaper sweep: \
         claimed={claimed} sent={sent} dead_lettered={dead_lettered}"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
