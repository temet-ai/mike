use chrono::{DateTime, Duration, Utc};
use serde_json::{Value as JsonValue, json};
use sqlx::PgPool;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::rehydrate::text_search::{
    BodySource, FindTextQuery, find_text_candidates,
};
use crate::store::persistence::request_write::IdMode;
use crate::store::persistence::test_support::TestDb;

async fn capture_at(
    persist: &PersistHandle,
    original: &JsonValue,
    sent: &JsonValue,
    at: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let mut req = crate::store::tests::make_test_request_with_timestamp(None, &session_id, at);
    req.request_body = Some(bytes::Bytes::from(original.to_string()));
    req.sent_body = Some(bytes::Bytes::from(sent.to_string()));
    Ok(persist
        .insert_request_initial(&req, true, IdMode::Mint)
        .await?
        .id)
}

async fn candidates(
    pool: &PgPool,
    needle: &str,
    source: BodySource,
    around: DateTime<Utc>,
) -> anyhow::Result<Vec<i64>> {
    let rows = find_text_candidates(
        pool,
        needle.as_bytes(),
        FindTextQuery {
            source,
            from: around - Duration::hours(1),
            to: around + Duration::hours(1),
            candidate_cap: 200,
        },
    )
    .await?;
    Ok(rows.into_iter().map(|row| row.request_id).collect())
}

fn conversation(first: &str) -> JsonValue {
    json!({
        "model": "m",
        "system": [{"type": "text", "text": "plain system"}],
        "messages": [
            {"role": "user", "content": first},
            {"role": "assistant", "content": "ok"},
            {"role": "user", "content": "next"}
        ],
    })
}

#[tokio::test]
async fn sent_candidates_needle_only_in_a_sent_override_found_without_request_body()
-> anyhow::Result<()> {
    let Some(db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = db.acquire().await?;
    let persist = PersistHandle::connect(&handle.database_url).await?;
    let pool = persist.pool();
    let needle = format!("sentneedle-{}", uuid::Uuid::new_v4());
    let at = Utc::now() - Duration::days(30);

    let overridden_original = conversation(&format!("original {needle}"));
    let overridden_sent = conversation("[read removed]");
    let overridden = capture_at(&persist, &overridden_original, &overridden_sent, at).await?;
    let hit = capture_at(&persist, &conversation("hi"), &conversation(&needle), at).await?;
    sqlx::query("UPDATE requests.requests SET request_body = NULL WHERE id = ANY($1)")
        .bind([overridden, hit].as_slice())
        .execute(pool)
        .await?;

    let sent = candidates(pool, &needle, BodySource::RequestSent, at).await?;
    assert_eq!(
        sent,
        vec![hit],
        "a sent override replaces the original message it covers"
    );
    let original = candidates(pool, &needle, BodySource::Request, at).await?;
    assert_eq!(original, vec![overridden]);
    handle.cleanup().await
}
