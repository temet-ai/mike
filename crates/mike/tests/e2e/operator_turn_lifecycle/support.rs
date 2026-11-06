use crate::anthropic_fixtures;

use std::time::Duration;

use anthropic_fixtures::{MockResponse, RequestBody, assistant_message, user_message};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub(crate) async fn start_upstream_stub() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new("msg_c8_lifecycle").minimal().build()),
        )
        .mount(&server)
        .await;
    server
}

pub(crate) fn user_prompt_body(prompt_text: &str) -> Value {
    RequestBody::default().content(prompt_text).build()
}

pub(crate) fn suggestion_probe_body(prior_prompt: &str, prior_reply: &str) -> Value {
    RequestBody::default()
        .messages(json!([
            user_message(prior_prompt),
            assistant_message(prior_reply),
            user_message("[SUGGESTION MODE: predict the operator's next likely message]"),
        ]))
        .build()
}

pub(crate) async fn drive_body(
    proxy_url: &str,
    session_id: &str,
    body: &Value,
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("content-type", "application/json")
        .json(body)
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "expected 2xx for proxied request, got {}",
        resp.status()
    );
    let _ = resp.bytes().await?;
    Ok(())
}

pub(crate) async fn drive_user_prompt(
    proxy_url: &str,
    session_id: &str,
    prompt_text: &str,
) -> anyhow::Result<()> {
    drive_body(proxy_url, session_id, &user_prompt_body(prompt_text)).await
}

pub(crate) async fn wait_for_request_rows(
    pool: &PgPool,
    session_id: &str,
    expected: usize,
) -> anyhow::Result<Vec<(i64, DateTime<Utc>, String, String)>> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let rows: Vec<(i64, DateTime<Utc>, String, String)> = sqlx::query_as(
            "SELECT r.id, r.created_at, rtt.code AS turn_type, rto.code AS turn_origin
               FROM requests.requests r
               JOIN sessions.sessions s ON s.id = r.session_id
               JOIN requests.turn_types rtt ON rtt.id = r.turn_type_id
               JOIN requests.turn_origins rto ON rto.id = r.turn_origin_id
              WHERE s.session_uuid = $1::uuid
              ORDER BY r.created_at ASC, r.id ASC",
        )
        .bind(session_id)
        .fetch_all(pool)
        .await?;
        if rows.len() >= expected {
            return Ok(rows);
        }
        if std::time::Instant::now() > deadline {
            anyhow::bail!(
                "timed out waiting for {expected} request rows on session={session_id}; saw {}",
                rows.len()
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[derive(Debug)]
pub(crate) struct TurnSnapshot {
    pub(crate) turn_id: i64,
    pub(crate) session_id: String,
    pub(crate) start_request_id: i64,
    pub(crate) end_request_id: i64,
    pub(crate) closed_at: Option<DateTime<Utc>>,
    pub(crate) classified_at: Option<DateTime<Utc>>,
    pub(crate) outcome: String,
    pub(crate) outcome_evidence_request_id: Option<i64>,
}

pub(crate) async fn list_turns_for_session(
    pool: &PgPool,
    session_id: &str,
) -> anyhow::Result<Vec<TurnSnapshot>> {
    let rows: Vec<TurnSnapshot> = sqlx::query_as::<
        _,
        (
            i64,
            String,
            i64,
            i64,
            Option<DateTime<Utc>>,
            Option<DateTime<Utc>>,
            String,
            Option<i64>,
        ),
    >(
        "SELECT ot.id, s.session_uuid::text AS session_id, ot.start_request_id, ot.end_request_id,
                ot.closed_at, ot.classified_at, toc.code AS outcome, ot.outcome_evidence_request_id
           FROM memory.operator_turns ot
           JOIN sessions.sessions s ON s.id = ot.session_id
           JOIN memory.turn_outcomes toc ON toc.id = ot.outcome_id
          WHERE s.session_uuid = $1::uuid
          ORDER BY ot.start_request_id ASC, ot.id ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(
        |(
            turn_id,
            session_id,
            start_request_id,
            end_request_id,
            closed_at,
            classified_at,
            outcome,
            outcome_evidence_request_id,
        )| TurnSnapshot {
            turn_id,
            session_id,
            start_request_id,
            end_request_id,
            closed_at,
            classified_at,
            outcome,
            outcome_evidence_request_id,
        },
    )
    .collect();
    Ok(rows)
}
