use std::sync::Arc;

use tokio::sync::RwLock;

use crate::store::RequestStore;
use crate::store::models::{InterceptedRequest, RequestId, RequestStatus, ToolResult};
use crate::store::persistence::test_support::TestDb;
use crate::store::persistence::{IdMode, PersistHandle};
use crate::types::ToolCall;

async fn shared_handle() -> anyhow::Result<Option<(PersistHandle, String)>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some((handle, db.database_url.clone())))
}

const NUL_BODY: &str = r#"{"model":"claude","messages":[{"role":"assistant","content":[{"type":"tool_use","id":"TOOL_ID","name":"Read","input":{"file_path":"nul\u0000.bin"}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"TOOL_ID","content":"a\u0000b"}]}]}"#;

fn nul_request(raw_id: u64, session_id: &str, tool_id: &str) -> InterceptedRequest {
    let mut req =
        crate::store::tests::make_test_request(Some(RequestId::from_raw(raw_id)), session_id);
    req.request_body = Some(bytes::Bytes::from(NUL_BODY.replace("TOOL_ID", tool_id)));
    req.message_count = 2;
    req.human_prompt = "read \0 this".to_string();
    req.last_user_message = "a\0b".to_string();
    req.tool_calls = vec![ToolCall {
        tool_id: tool_id.to_string(),
        name: "Read".to_string(),
        input_json: r#"{"file_path":"nul\u0000.bin"}"#.to_string(),
        complete: true,
    }];
    req.tool_results_in_body = vec![ToolResult {
        tool_use_id: tool_id.to_string(),
        is_error: false,
        content_sha256: "0".repeat(64),
        content_bytes: 3,
        content: serde_json::Value::String("a\0b".to_string()),
    }];
    req
}

#[tokio::test]
async fn nul_in_tool_result_records_request_byte_for_byte_with_derived_rows() -> anyhow::Result<()>
{
    let Some((handle, _url)) = shared_handle().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let tool_id = format!("tu-nul-{session_id}");
    let mut req = nul_request(363_001, &session_id, &tool_id);
    req.completed_at = Some(chrono::Utc::now());
    handle
        .insert_request_initial(&req, true, IdMode::Ensure)
        .await?;
    handle.snapshot_request(&req).await?;

    let body: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT request_body FROM requests.requests WHERE id = $1")
            .bind(363_001_i64)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(body.as_deref(), req.request_body.as_deref());

    let (messages, uses, results, prompt): (i64, i64, String, String) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM requests.messages WHERE request_id = $1),
                (SELECT count(*) FROM tools.uses WHERE request_id = $1
                    AND input->>'file_path' = 'nul' || chr(9216) || '.bin'),
                (SELECT content #>> '{}' FROM tools.results WHERE request_id = $1),
                (SELECT human_prompt FROM requests.requests WHERE id = $1)",
    )
    .bind(363_001_i64)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(messages, 2, "both messages bridged");
    assert_eq!(uses, 1, "tool input stored with the stand-in");
    assert_eq!(results, "a\u{2400}b");
    assert_eq!(prompt, "read \u{2400} this");
    Ok(())
}

async fn seed_session(handle: &PersistHandle, session_id: &str) -> anyhow::Result<i64> {
    let mut first = crate::store::tests::make_test_request(None, session_id);
    first.request_body = Some(bytes::Bytes::from_static(
        br#"{"messages":[{"role":"user","content":"first"}]}"#,
    ));
    let outcome = handle
        .insert_request_initial(&first, true, IdMode::Mint)
        .await?;
    first.id = RequestId::from_raw(outcome.id as u64);
    first.completed_at = Some(chrono::Utc::now());
    handle.snapshot_request(&first).await?;
    Ok(
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::uuid")
            .bind(session_id)
            .fetch_one(handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn first_write_timing_out_on_a_session_lock_is_recorded_after_the_answer()
-> anyhow::Result<()> {
    let Some((handle, url)) = shared_handle().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let session_db_id = seed_session(&handle, &session_id).await?;

    let holder = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let mut lock_tx = holder.begin().await?;
    sqlx::query("SELECT 1 FROM sessions.sessions WHERE id = $1 FOR UPDATE")
        .bind(session_db_id)
        .execute(&mut *lock_tx)
        .await?;

    let mut store = RequestStore::new(crate::store::tests::small_policy(10));
    store.set_persist(handle.clone());
    let store = Arc::new(RwLock::new(store));
    let mut req = crate::store::tests::make_test_request(None, &session_id);
    req.request_body = Some(bytes::Bytes::from_static(
        br#"{"messages":[{"role":"user","content":"late"}]}"#,
    ));
    req.status = RequestStatus::Pending;
    let id = RequestStore::insert_request_stub_to_db(&store, &req, false).await;
    assert!(
        crate::store::models::is_fallback(id.as_u64()),
        "the first write must fail on the held lock"
    );
    assert!(super::is_deferred(id), "a timed-out first write is kept");
    lock_tx.rollback().await?;

    req.id = id;
    req.status = RequestStatus::Complete;
    req.response_body = Some(bytes::Bytes::from_static(b"{\"content\":[]}"));
    req.completed_at = Some(chrono::Utc::now());
    store.write().await.insert(req);
    RequestStore::snapshot_terminal(&store, id).await;

    let rows: Vec<(String, Option<i64>)> = sqlx::query_as(
        "SELECT rs.code, r.request_number
           FROM requests.requests r
           JOIN requests.statuses rs ON rs.id = r.status_id
          WHERE r.session_id = $1
          ORDER BY r.created_at, r.id",
    )
    .bind(session_db_id)
    .fetch_all(handle.pool())
    .await?;
    assert_eq!(
        rows,
        vec![
            ("complete".to_string(), Some(1)),
            ("complete".to_string(), Some(2)),
        ],
        "the late request is recorded, complete, after the first in send order"
    );
    assert!(
        !super::is_deferred(id),
        "a recorded request leaves the waiting list"
    );
    Ok(())
}
