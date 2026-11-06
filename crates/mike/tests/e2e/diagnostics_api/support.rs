use crate::anthropic_fixtures;

use anthropic_fixtures::RequestBody;

pub(super) async fn send_request(
    client: &reqwest::Client,
    proxy_url: &str,
    session_id: &str,
    user_text: &str,
) -> anyhow::Result<()> {
    let body = RequestBody::default()
        .max_tokens(4)
        .content(user_text)
        .build();
    let response = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    anyhow::ensure!(response.status() == 200);
    let _ = response.bytes().await?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(())
}

pub(super) struct DiagInsert<'a> {
    pub(super) request_id: i64,
    pub(super) category: &'a str,
    pub(super) analyzer: &'a str,
    pub(super) advice: &'a str,
    pub(super) matched_text: &'a str,
    pub(super) diag_created_at: chrono::DateTime<chrono::Utc>,
}

pub(super) async fn insert_diagnostic_full(
    pool: &sqlx::PgPool,
    d: DiagInsert<'_>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO requests.diagnostics
            (request_id, category, analyzer, advice,
             matched_text, match_range, other_hits, created_at)
         VALUES ($1, $2, $3, $4, $5, NULL, '[]'::jsonb, $6)
         ON CONFLICT DO NOTHING",
    )
    .bind(d.request_id)
    .bind(d.category)
    .bind(d.analyzer)
    .bind(d.advice)
    .bind(d.matched_text)
    .bind(d.diag_created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn set_human_prompt(
    pool: &sqlx::PgPool,
    request_id: i64,
    text: &str,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE requests.requests SET human_prompt = $1 WHERE id = $2")
        .bind(text)
        .bind(request_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub(super) async fn insert_diagnostic(
    pool: &sqlx::PgPool,
    request_id: i64,
    category: &str,
    analyzer: &str,
    diag_created_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<()> {
    insert_diagnostic_full(
        pool,
        DiagInsert {
            request_id,
            category,
            analyzer,
            advice: "advice text",
            matched_text: "match text",
            diag_created_at,
        },
    )
    .await
}
