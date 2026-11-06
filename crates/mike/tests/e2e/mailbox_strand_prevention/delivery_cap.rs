use sqlx::postgres::PgPoolOptions;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use super::support::{
    TestDb, boot_proxy, mailbox_row_state, ok_response, primary_body, queue_mailbox_message,
    send_request, wait_for_session_row,
};

#[tokio::test]
async fn turn_delivers_at_most_the_configured_cap() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping turn_delivers_at_most_the_configured_cap");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    const CAP: usize = 2;
    let (_state, proxy_url, inspect_url, shutdown) = boot_proxy(
        &mock.uri(),
        &db.database_url,
        &format!("mailbox_max_deliveries_per_turn = {CAP}\n"),
    )
    .await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    let session_str = session.to_string();
    send_request(&client, &proxy_url, &session_str, &primary_body("init")).await?;
    wait_for_session_row(&pool, session).await?;

    let texts: Vec<String> = (0..5)
        .map(|i| format!("cap-marker-{i}-distinctive"))
        .collect();
    let mut message_uuids = Vec::with_capacity(texts.len());
    for text in &texts {
        message_uuids.push(queue_mailbox_message(&client, &inspect_url, session, text).await?);
    }

    send_request(&client, &proxy_url, &session_str, &primary_body("resume")).await?;

    let received = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    let forwarded_bytes = &received
        .last()
        .ok_or_else(|| anyhow::anyhow!("no forwarded request"))?
        .body;
    let forwarded_text = String::from_utf8_lossy(forwarded_bytes);
    let delivered_in_body = texts
        .iter()
        .filter(|t| forwarded_text.contains(t.as_str()))
        .count();
    assert_eq!(
        delivered_in_body, CAP,
        "expected exactly {CAP} mailbox messages injected in the forwarded body; got {delivered_in_body} in: {forwarded_text}"
    );

    let mut delivered_rows = 0;
    let mut pending_rows = 0;
    for uuid in &message_uuids {
        let (claimed, delivered, _consumed) = mailbox_row_state(&pool, *uuid).await?;
        if delivered {
            delivered_rows += 1;
        } else {
            assert!(
                !claimed,
                "an undelivered row past the cap must be unclaimed (not stuck mid-claim): {uuid}"
            );
            pending_rows += 1;
        }
    }
    assert_eq!(delivered_rows, CAP, "exactly CAP rows should be delivered");
    assert_eq!(
        pending_rows,
        texts.len() - CAP,
        "the rest must remain queued for a later turn"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
