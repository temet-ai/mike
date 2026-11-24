use anyhow::Context;
use serde_json::{Value as JsonValue, json};

use super::{SentColumns, clear_request_body, sent_columns, sent_of, store_request_body};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::messages::{SentPatchRow, apply_sent_patches};
use crate::store::persistence::test_support;

const RAW: &str = r#"{"messages":[{"content":[{"text":"turn 0","type":"text"}],"role":"user"},{"content":"turn 1","role":"assistant"},{"content":[{"text":"turn 2","type":"text"}],"role":"user"}],"model":"m","stream":true}"#;

async fn capture_raw(handle: &PersistHandle) -> anyhow::Result<i64> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let mut req = crate::store::tests::make_test_request(None, &session_id);
    req.request_body = Some(bytes::Bytes::from_static(RAW.as_bytes()));
    let outcome = handle
        .insert_request_initial(
            &req,
            true,
            crate::store::persistence::request_write::IdMode::Mint,
        )
        .await?;
    store_request_body(handle, outcome.id, RAW.as_bytes()).await?;
    Ok(outcome.id)
}

fn patch(raw: &str, from: &str, to: &str, bytes: &str) -> anyhow::Result<SentPatchRow> {
    let start = raw.find(from).context("patch start")?;
    let end = raw[start..].find(to).context("patch end")? + start + to.len();
    Ok(SentPatchRow {
        sent_patch_start: i32::try_from(start)?,
        sent_patch_end: i32::try_from(end)?,
        sent_patch_bytes: bytes.as_bytes().to_vec(),
    })
}

async fn store_old_patches(
    handle: &PersistHandle,
    request_id: i64,
    envelope: &SentPatchRow,
    message_zero: &SentPatchRow,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE requests.requests
            SET sent_patch_start = $2, sent_patch_end = $3, sent_patch_bytes = $4
          WHERE id = $1",
    )
    .bind(request_id)
    .bind(envelope.sent_patch_start)
    .bind(envelope.sent_patch_end)
    .bind(&envelope.sent_patch_bytes)
    .execute(&handle.pool)
    .await?;
    let updated = sqlx::query(
        "UPDATE requests.deltas
            SET sent_patch_start = $2, sent_patch_end = $3, sent_patch_bytes = $4
          WHERE request_id = $1 AND ord = 0
            AND change_kind_id = (SELECT id FROM requests.delta_change_kinds WHERE code = 'added')",
    )
    .bind(request_id)
    .bind(message_zero.sent_patch_start)
    .bind(message_zero.sent_patch_end)
    .bind(&message_zero.sent_patch_bytes)
    .execute(&handle.pool)
    .await?
    .rows_affected();
    anyhow::ensure!(
        updated == 1,
        "the first request's ord 0 must have an added delta row"
    );
    Ok(())
}

async fn seed_patched_row(handle: &PersistHandle) -> anyhow::Result<(i64, JsonValue)> {
    let id = capture_raw(handle).await?;
    let raw = RAW;
    let straddling = patch(
        raw,
        "turn 2",
        r#""model":"m""#,
        r#"turn 2 EDITED","type":"text"}],"role":"user"}],"model":"m2""#,
    )?;
    let message_zero = patch(
        raw,
        r#"{"content":[{"text":"turn 0""#,
        r#""role":"user"}"#,
        r#"{"content":[{"text":"[read removed]","type":"text"}],"role":"user"}"#,
    )?;
    anyhow::ensure!(
        message_zero.sent_patch_end <= straddling.sent_patch_start,
        "the fixture's patches must not overlap"
    );
    store_old_patches(handle, id, &straddling, &message_zero).await?;
    let true_sent = apply_sent_patches(raw.as_bytes(), vec![message_zero, straddling])?;
    Ok((id, serde_json::from_slice(&true_sent)?))
}

#[tokio::test]
async fn backfill_old_patches_rebuild_the_true_sent_body_without_request_body() -> anyhow::Result<()>
{
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (id, true_sent) = seed_patched_row(&handle).await?;
    assert_eq!(true_sent["model"], json!("m2"));
    assert_eq!(
        true_sent["messages"][2]["content"][0]["text"],
        json!("turn 2 EDITED")
    );

    let dry = handle
        .backfill_sent_content_batch(id, Some(id), 10, false)
        .await?
        .context("the patched row is a candidate")?;
    assert_eq!((dry.scanned, dry.verified, dry.failures.len()), (1, 1, 0));
    let none = SentColumns {
        has_envelope: false,
        count: None,
        ords: vec![],
    };
    assert_eq!(
        sent_columns(&handle, id).await?,
        none,
        "a dry run writes nothing"
    );

    let done = handle
        .backfill_sent_content_batch(id, Some(id), 10, true)
        .await?
        .context("the patched row is still a candidate")?;
    assert_eq!((done.verified, done.failures.len()), (1, 0));
    let written = SentColumns {
        has_envelope: true,
        count: None,
        ords: vec![0, 2],
    };
    assert_eq!(sent_columns(&handle, id).await?, written);

    clear_request_body(&handle, id).await?;
    assert_eq!(sent_of(&handle, id).await?, true_sent);
    let rerun = handle
        .backfill_sent_content_batch(id, Some(id), 10, true)
        .await?;
    assert_eq!(rerun, None, "a backfilled row is no longer a candidate");
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backfill_patches_that_do_not_apply_are_reported_and_left_alone() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let id = capture_raw(&handle).await?;
    sqlx::query(
        "UPDATE requests.requests
            SET sent_patch_start = 0, sent_patch_end = 100000, sent_patch_bytes = 'x'::bytea
          WHERE id = $1",
    )
    .bind(id)
    .execute(&handle.pool)
    .await?;

    let batch = handle
        .backfill_sent_content_batch(id, Some(id), 10, true)
        .await?
        .context("the patched row is a candidate")?;
    assert_eq!(batch.verified, 0);
    assert_eq!(batch.failures.len(), 1);
    assert_eq!(batch.failures[0].request_id, id);
    let none = SentColumns {
        has_envelope: false,
        count: None,
        ords: vec![],
    };
    assert_eq!(sent_columns(&handle, id).await?, none);
    let (start,): (Option<i32>,) =
        sqlx::query_as("SELECT sent_patch_start FROM requests.requests WHERE id = $1")
            .bind(id)
            .fetch_one(&handle.pool)
            .await?;
    assert_eq!(start, Some(0), "the old patch is kept");
    db.cleanup().await?;
    Ok(())
}
