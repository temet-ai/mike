use anyhow::Context;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::request_write::IdMode;
use crate::store::persistence::test_support;

const PLAIN: &str = r#"{"model":"m","messages":[{"role":"user","content":"a"}],"tail":"T"}"#;
const SPACED: &str = r#"{"model":"m","messages":[ {"role": "user", "content": "a"} ],"tail":"T"}"#;

async fn capture_raw(
    handle: &PersistHandle,
    session_id: &str,
    raw: &'static str,
) -> anyhow::Result<(i64, i64)> {
    let mut req = crate::store::tests::make_test_request(None, session_id);
    req.request_body = Some(bytes::Bytes::from_static(raw.as_bytes()));
    let outcome = handle
        .insert_request_initial(&req, true, IdMode::Mint)
        .await?;
    Ok((
        outcome.id,
        outcome.session_id.context("the capture has a session")?,
    ))
}

async fn body_kept(handle: &PersistHandle, request_id: i64) -> anyhow::Result<bool> {
    let (kept,): (bool,) =
        sqlx::query_as("SELECT request_body IS NOT NULL FROM requests.requests WHERE id = $1")
            .bind(request_id)
            .fetch_one(&handle.pool)
            .await?;
    Ok(kept)
}

#[tokio::test]
async fn compaction_keeps_a_body_whose_sent_version_does_not_rebuild() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let (plain, session_db_id) = capture_raw(&handle, &session_id, PLAIN).await?;
    let (spaced, _) = capture_raw(&handle, &session_id, SPACED).await?;
    let (last, _) = capture_raw(&handle, &session_id, PLAIN).await?;
    let tail = i32::try_from(SPACED.find(r#""T""#).context("tail value")?)?;
    sqlx::query(
        "UPDATE requests.requests
            SET sent_patch_start = $2, sent_patch_end = $3, sent_patch_bytes = '\"T2\"'::bytea
          WHERE id = $1",
    )
    .bind(spaced)
    .bind(tail)
    .bind(tail + 3)
    .execute(&handle.pool)
    .await?;
    sqlx::query(
        "UPDATE sessions.sessions SET last_seen = now() - interval '30 days' WHERE id = $1",
    )
    .bind(session_db_id)
    .execute(&handle.pool)
    .await?;

    let outcome = handle.compact_request_bodies(14, 100, 0..=i64::MAX).await?;
    assert_eq!((outcome.cleared_rows, outcome.refused_rows), (1, 1));
    assert!(
        !body_kept(&handle, plain).await?,
        "a row that rebuilds is cleared"
    );
    assert!(
        body_kept(&handle, spaced).await?,
        "a row whose old patch does not apply to the rebuilt body keeps its body"
    );
    assert!(
        body_kept(&handle, last).await?,
        "the session's final request keeps its body"
    );

    let backfilled = handle
        .backfill_sent_content_batch(spaced, Some(spaced), 10, true)
        .await?
        .context("the refused row is a backfill candidate")?;
    assert_eq!((backfilled.verified, backfilled.failures.len()), (1, 0));
    let again = handle.compact_request_bodies(14, 100, 0..=i64::MAX).await?;
    assert_eq!((again.cleared_rows, again.refused_rows), (1, 0));
    assert!(
        !body_kept(&handle, spaced).await?,
        "after the backfill the row rebuilds"
    );
    db.cleanup().await?;
    Ok(())
}
