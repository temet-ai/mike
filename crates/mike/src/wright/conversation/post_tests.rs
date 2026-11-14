use axum::body::Bytes;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use serde_json::{Value, json};

use super::fakes_tests::{Db, TOKEN, bearer, operator_token};
use crate::api::wright::conversation::post_message_with;
use crate::store::persistence::PersistHandle;

async fn read(response: Response) -> anyhow::Result<(StatusCode, Value)> {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    ))
}

async fn post(
    handle: &PersistHandle,
    headers: &HeaderMap,
    body: &Value,
) -> anyhow::Result<(StatusCode, Value)> {
    let token = operator_token()?;
    let bytes = Bytes::from(serde_json::to_vec(body)?);
    match post_message_with(handle, Some(&token), headers, &bytes).await {
        Ok(response) | Err(response) => read(response).await,
    }
}

async fn stored(db: &Db, client_msg_id: &str) -> anyhow::Result<(i64, i64)> {
    Ok(sqlx::query_as(
        "SELECT count(DISTINCT m.id), count(j.id)
           FROM wright.messages m LEFT JOIN wright.reply_jobs j ON j.message_id = m.id
          WHERE m.client_msg_id = $1",
    )
    .bind(client_msg_id)
    .fetch_one(db.handle.pool())
    .await?)
}

#[tokio::test]
async fn post_without_a_valid_token_is_401_and_stores_nothing() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let body = json!({"client_msg_id": "no-token", "body": "leave the judge alone today"});
    let (status, answer) = post(&db.handle, &HeaderMap::new(), &body).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{answer}");
    let (status, _) = post(&db.handle, &bearer("0000")?, &body).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let bytes = Bytes::from(serde_json::to_vec(&body)?);
    let unpaired = match post_message_with(&db.handle, None, &bearer(TOKEN)?, &bytes).await {
        Ok(response) | Err(response) => response.status(),
    };
    assert_eq!(unpaired, StatusCode::UNAUTHORIZED);
    assert_eq!(stored(&db, "no-token").await?, (0, 0));
    db.cleanup().await
}

#[tokio::test]
async fn post_with_only_a_sender_field_host_or_origin_is_401() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let mut claims = HeaderMap::new();
    claims.insert(header::HOST, HeaderValue::from_static("localhost:9989"));
    claims.insert(
        header::ORIGIN,
        HeaderValue::from_static("http://localhost:9990"),
    );
    claims.insert(
        header::REFERER,
        HeaderValue::from_static("http://localhost:9990/wright"),
    );
    let body = json!({
        "client_msg_id": "claims-only",
        "body": "pause every fix",
        "sender": "operator",
        "author": "operator",
    });
    let (status, answer) = post(&db.handle, &claims, &body).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{answer}");
    assert_eq!(answer["error"], "unauthorized");
    assert_eq!(stored(&db, "claims-only").await?, (0, 0));
    db.cleanup().await
}

#[tokio::test]
async fn post_retried_with_the_same_client_msg_id_stores_one_message() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let headers = bearer(TOKEN)?;
    let body = json!({"client_msg_id": "retry-1", "body": "why did #4 open?"});
    let (first, created) = post(&db.handle, &headers, &body).await?;
    assert_eq!(first, StatusCode::CREATED, "{created}");
    assert_eq!(created["message"]["author"], "operator");
    assert_eq!(created["message"]["steer"], Value::Null);
    let (second, again) = post(&db.handle, &headers, &body).await?;
    assert_eq!(second, StatusCode::OK, "{again}");
    assert_eq!(again["created"], false);
    assert_eq!(again["message"]["id"], created["message"]["id"]);
    assert_eq!(stored(&db, "retry-1").await?, (1, 1));
    let changed = json!({"client_msg_id": "retry-1", "body": "something else"});
    let (status, conflict) = post(&db.handle, &headers, &changed).await?;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
    assert_eq!(conflict["error"], "conflict");
    assert_eq!(stored(&db, "retry-1").await?, (1, 1));
    db.cleanup().await
}

#[tokio::test]
async fn post_with_bad_input_names_the_field_and_value() -> anyhow::Result<()> {
    let db = Db::boot().await?;
    let headers = bearer(TOKEN)?;
    let (status, answer) = post(
        &db.handle,
        &headers,
        &json!({"client_msg_id": "", "body": "hi"}),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        answer["error"]
            .as_str()
            .is_some_and(|e| e.contains("client_msg_id")),
        "{answer}"
    );
    let (status, answer) = post(
        &db.handle,
        &headers,
        &json!({"client_msg_id": "dangling", "body": "hi", "reply_to": 999_999}),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        answer["error"]
            .as_str()
            .is_some_and(|e| e.contains("999999")),
        "{answer}"
    );
    db.cleanup().await
}
