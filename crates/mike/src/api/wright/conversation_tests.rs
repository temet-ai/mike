use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use serde_json::Value;

use super::conversation::{authorization, authorize_headers};
use crate::wright::conversation::fakes_tests::{TOKEN, operator_token};

pub(super) fn bearer(token: &str) -> anyhow::Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))?,
    );
    Ok(headers)
}

pub(super) async fn read(response: Response) -> anyhow::Result<(StatusCode, Value)> {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    ))
}

#[test]
fn authorize_headers_without_a_bearer_or_with_identity_headers_only_is_refused()
-> anyhow::Result<()> {
    let token = operator_token()?;
    assert!(authorize_headers(&HeaderMap::new(), Some(&token)).is_err());
    let mut claims = HeaderMap::new();
    claims.insert(header::HOST, HeaderValue::from_static("localhost:9990"));
    claims.insert(
        header::ORIGIN,
        HeaderValue::from_static("http://localhost:9990"),
    );
    claims.insert("x-sender", HeaderValue::from_static("operator"));
    assert_eq!(authorization(&claims), None);
    assert!(authorize_headers(&claims, Some(&token)).is_err());
    assert!(authorize_headers(&bearer("not-the-token")?, Some(&token)).is_err());
    assert!(authorize_headers(&bearer(TOKEN)?, None).is_err());
    Ok(())
}

#[test]
fn authorize_headers_with_the_operator_bearer_passes_in_any_scheme_case() -> anyhow::Result<()> {
    let token = operator_token()?;
    assert!(authorize_headers(&bearer(TOKEN)?, Some(&token)).is_ok());
    let mut lower = HeaderMap::new();
    lower.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("bearer {TOKEN}"))?,
    );
    assert!(authorize_headers(&lower, Some(&token)).is_ok());
    let mut basic = HeaderMap::new();
    basic.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Basic {TOKEN}"))?,
    );
    assert!(authorize_headers(&basic, Some(&token)).is_err());
    Ok(())
}

#[tokio::test]
async fn unauthorized_answers_401_with_a_bearer_challenge_and_the_reason() -> anyhow::Result<()> {
    let token = operator_token()?;
    let Err(refusal) = authorize_headers(&HeaderMap::new(), Some(&token)) else {
        anyhow::bail!("a request without a bearer was authorized");
    };
    let response = refusal.into_response();
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok()),
        Some("Bearer")
    );
    let (status, body) = read(response).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "unauthorized");
    assert!(
        body["reason"]
            .as_str()
            .is_some_and(|r| r.contains("Authorization: Bearer")),
        "{body}"
    );
    Ok(())
}
