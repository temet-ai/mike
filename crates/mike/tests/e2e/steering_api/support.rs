use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody};

pub(super) async fn fire_request_and_get_id(
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

pub(super) async fn seed_intervention(
    pool: &sqlx::PgPool,
    session_uuid: &str,
    trigger_request_id: i64,
    anchor_text: &str,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO steering.interventions
             (steerer_id, enrolment_id, session_id, verdict_id, mode_id, authority_id,
              trigger_request_id, sent_request_id, off_thread_topic, confidence,
              payload_text, anchor_text, model, sent_at, processed_at)
         SELECT st.id, NULL, s.id, v.id, m.id, a.id,
                $2, $2, 'pagination test row', 0.9,
                'pagination test payload', $3, 'test-model', now(), now()
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
    .bind(session_uuid)
    .bind(trigger_request_id)
    .bind(anchor_text)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn mount_minimal_upstream(msg_id: &str) -> wiremock::MockServer {
    let upstream = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/messages"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new(msg_id).minimal().typed().build()),
        )
        .mount(&upstream)
        .await;
    upstream
}

pub(super) async fn fire_ensures_session_exists(
    client: &reqwest::Client,
    proxy_url: &str,
    session_uuid: &str,
    prompt: &str,
) -> anyhow::Result<()> {
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
    Ok(())
}
