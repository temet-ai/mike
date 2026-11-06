use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::{MockResponse, RequestBody};
use db_helpers::ProxyWithDb;
use serde_json::Value;

const INTERVENTION_MODEL: &str = "openrouter/google/gemini-3.5-flash-lite";

async fn fire_request_and_get_id(
    client: &reqwest::Client,
    proxy_url: &str,
    pool: &sqlx::PgPool,
    session_uuid: &str,
    prompt: &str,
) -> anyhow::Result<i64> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_uuid)
        .header("content-type", "application/json")
        .json(
            &RequestBody::default()
                .content(prompt)
                .max_tokens(16)
                .build(),
        )
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "proxy request failed: {}",
        resp.status()
    );
    let _ = resp.bytes().await?;

    for _ in 0..50 {
        if let Some(id) = sqlx::query_scalar::<_, i64>(
            "SELECT r.id FROM requests.requests r
               JOIN sessions.sessions s ON s.id = r.session_id
              WHERE s.session_uuid  = $1::uuid
                AND r.human_prompt  = $2
              ORDER BY r.id DESC
              LIMIT 1",
        )
        .bind(session_uuid)
        .bind(prompt)
        .fetch_optional(pool)
        .await?
        {
            return Ok(id);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    anyhow::bail!("request for prompt {prompt:?} never appeared in requests.requests")
}

struct SeedInterventionParams<'a> {
    session_uuid: &'a str,
    trigger_request_id: i64,
    sent_request_id: i64,
    off_thread_topic: &'a str,
    confidence: f64,
    payload_text: &'a str,
    anchor_text: &'a str,
    model: &'a str,
}

async fn seed_intervention(
    pool: &sqlx::PgPool,
    p: SeedInterventionParams<'_>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO steering.interventions
             (steerer_id, enrolment_id, session_id, verdict_id, mode_id, authority_id,
              trigger_request_id, sent_request_id, off_thread_topic, confidence,
              payload_text, anchor_text, model, sent_at, processed_at)
         SELECT st.id, NULL, s.id, v.id, m.id, a.id,
                $2, $3, $4, $5, $6, $7, $8, now(), now()
           FROM steering.steerers    st,
                steering.verdicts    v,
                steering.modes       m,
                steering.authorities a,
                sessions.sessions    s
          WHERE st.name    = 'verifier'
            AND v.code     = 'fired'
            AND m.code     = 'augment'
            AND a.code     = 'redirect'
            AND s.session_uuid = $1::uuid
         RETURNING id",
    )
    .bind(p.session_uuid)
    .bind(p.trigger_request_id)
    .bind(p.sent_request_id)
    .bind(p.off_thread_topic)
    .bind(p.confidence)
    .bind(p.payload_text)
    .bind(p.anchor_text)
    .bind(p.model)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn seed_delivered_intervention(
    client: &reqwest::Client,
    env: &ProxyWithDb,
    session_uuid: &str,
) -> anyhow::Result<(i64, i64)> {
    let trigger_request_id = fire_request_and_get_id(
        client,
        &env.proxy_url,
        &env.pool,
        session_uuid,
        "request-steering-api-test trigger prompt",
    )
    .await?;
    let sent_request_id = fire_request_and_get_id(
        client,
        &env.proxy_url,
        &env.pool,
        session_uuid,
        "request-steering-api-test sent prompt",
    )
    .await?;
    anyhow::ensure!(
        trigger_request_id != sent_request_id,
        "trigger and sent requests must be distinct captured rows"
    );

    seed_intervention(
        &env.pool,
        SeedInterventionParams {
            session_uuid,
            trigger_request_id,
            sent_request_id,
            off_thread_topic: "session drifted into an unrelated refactor",
            confidence: 0.87,
            payload_text: "You are drifting off-thread; refocus on the worklog's Next line.",
            anchor_text: "State of play / Next: finish the steering endpoint.",
            model: INTERVENTION_MODEL,
        },
    )
    .await?;

    Ok((trigger_request_id, sent_request_id))
}

fn assert_delivered_attribution_body(
    body: &Value,
    trigger_request_id: i64,
    sent_request_id: i64,
    session_uuid: &str,
) -> anyhow::Result<()> {
    assert_eq!(body["steerer_name"], "verifier");
    assert_eq!(body["steerer_display_name"], "Verifier");
    assert_eq!(body["steerer_model"], INTERVENTION_MODEL);
    assert_eq!(body["authority"], "redirect");
    assert_eq!(body["mode"], "augment");
    assert_eq!(body["verdict"], "fired");
    assert_eq!(
        body["off_thread_topic"],
        "session drifted into an unrelated refactor"
    );
    assert_eq!(
        body["payload_text"],
        "You are drifting off-thread; refocus on the worklog's Next line."
    );
    assert_eq!(
        body["anchor_text"],
        "State of play / Next: finish the steering endpoint."
    );
    assert_eq!(body["trigger_request_id"], trigger_request_id);
    assert_eq!(body["sent_request_id"], sent_request_id);
    assert_eq!(body["session_uuid"], session_uuid);
    let confidence = body["confidence"].as_f64().ok_or_else(|| {
        anyhow::anyhow!("confidence must be a number, got {:?}", body["confidence"])
    })?;
    assert!(
        (confidence - 0.87).abs() < 1e-9,
        "expected confidence 0.87, got {confidence}"
    );
    Ok(())
}

#[tokio::test]
async fn get_request_steering_delivered_request_returns_attribution() -> anyhow::Result<()> {
    let upstream = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/messages"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(
                    MockResponse::new("msg_test_request_steering")
                        .minimal()
                        .typed()
                        .build(),
                ),
        )
        .mount(&upstream)
        .await;

    let Some(env) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             get_request_steering_delivered_request_returns_attribution"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let session_uuid = "c0ffee00-0000-0000-0000-0000000000a1";

    let (trigger_request_id, sent_request_id) =
        seed_delivered_intervention(&client, &env, session_uuid).await?;

    let resp = client
        .get(format!(
            "{}/api/requests/{sent_request_id}/steering",
            env.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "expected 200 for a request carrying a delivered intervention; got {}",
        resp.status()
    );
    let body: Value = resp.json().await?;
    assert_delivered_attribution_body(&body, trigger_request_id, sent_request_id, session_uuid)?;

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}

#[tokio::test]
async fn get_request_steering_non_steered_request_returns_404() -> anyhow::Result<()> {
    let upstream = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/messages"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(
                    MockResponse::new("msg_test_request_steering_404")
                        .minimal()
                        .typed()
                        .build(),
                ),
        )
        .mount(&upstream)
        .await;

    let Some(env) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             get_request_steering_non_steered_request_returns_404"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let session_uuid = "c0ffee00-0000-0000-0000-0000000000a2";

    let plain_request_id = fire_request_and_get_id(
        &client,
        &env.proxy_url,
        &env.pool,
        session_uuid,
        "request-steering-api-test unsteered prompt",
    )
    .await?;

    let resp = client
        .get(format!(
            "{}/api/requests/{plain_request_id}/steering",
            env.inspect_url
        ))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "expected 404 for a request with no delivered intervention; got {}",
        resp.status()
    );

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}
