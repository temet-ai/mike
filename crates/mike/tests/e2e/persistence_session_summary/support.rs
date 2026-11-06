use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub(crate) async fn send_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    user_text: &str,
) -> anyhow::Result<bytes::Bytes> {
    let body = RequestBody::default()
        .content(user_text)
        .max_tokens(16)
        .build();
    let response = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(
        response.status() == 200,
        "expected 200 OK, got {}",
        response.status()
    );
    Ok(response.bytes().await?)
}

pub(crate) async fn mock_upstream_canned() -> MockServer {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_phase1")
                        .text("ack")
                        .usage(5, 1)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;
    mock_server
}

#[derive(sqlx::FromRow)]
pub(crate) struct SessionRowSnapshot {
    pub(crate) request_count: i32,
    pub(crate) last_status: Option<String>,
    pub(crate) last_models: Vec<String>,
    pub(crate) project_path: String,
    pub(crate) last_seen: chrono::DateTime<chrono::Utc>,
    pub(crate) first_seen: chrono::DateTime<chrono::Utc>,
}

pub(crate) async fn fetch_session_row(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> anyhow::Result<Option<SessionRowSnapshot>> {
    let row: Option<SessionRowSnapshot> = sqlx::query_as(
        "SELECT v.request_count,
                v.last_status,
                v.last_models::text[] AS last_models,
                v.last_seen,
                v.first_seen,
                v.project_path
         FROM mike.session_summary_v v
         WHERE v.session_id = $1::text",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub(crate) async fn count_request_rows(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> anyhow::Result<i64> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::bigint FROM requests.requests r
          JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::uuid",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

pub(crate) async fn wait_for_persisted_rows(
    pool: &sqlx::PgPool,
    session_id: &str,
    min_count: i64,
) -> anyhow::Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let n = count_request_rows(pool, session_id).await?;
        if n >= min_count {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for {min_count} persisted rows on session {session_id}; saw {n}"
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
