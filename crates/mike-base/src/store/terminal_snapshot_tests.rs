use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::store::RequestStore;
use crate::store::models::{RequestId, RequestStatus};
use crate::store::persistence::new_session_hook::NewSessionHook;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use crate::store::persistence::{PersistError, PersistHandle};
use crate::store::tests::{make_test_request, small_policy};
use crate::types::ToolCall;

#[derive(Default)]
struct CountingHook(AtomicUsize);

impl NewSessionHook for CountingHook {
    fn on_new_session<'a>(
        &'a self,
        _persist: &'a PersistHandle,
        _session: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<(), PersistError>> + Send + 'a>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
}

struct Fixture {
    store: Arc<RwLock<RequestStore>>,
    hook: Arc<CountingHook>,
    pool: sqlx::PgPool,
    db: TestDbHandle,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let hook = Arc::new(CountingHook::default());
    handle.set_new_session_hook(hook.clone());
    let mut store = RequestStore::new(small_policy(10));
    store.set_persist(handle);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    Ok(Some(Fixture {
        store: Arc::new(RwLock::new(store)),
        hook,
        pool,
        db,
    }))
}

async fn captured(fx: &Fixture, session: &str, calls: Vec<ToolCall>) -> RequestId {
    let mut req = make_test_request(None, session);
    let is_new = fx.store.read().await.session_is_new(session);
    let id = RequestStore::capture_request_stub(&fx.store, &mut req, is_new).await;
    req.status = RequestStatus::Complete;
    req.completed_at = Some(Utc::now());
    req.tool_calls = calls;
    fx.store.write().await.insert(req);
    id
}

fn call(tool_id: &str, complete: bool) -> ToolCall {
    ToolCall {
        tool_id: tool_id.to_owned(),
        name: "Read".to_owned(),
        input_json: r#"{"file_path":"/tmp/x"}"#.to_owned(),
        complete,
    }
}

async fn tool_use_count(pool: &sqlx::PgPool, request_id: RequestId) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM tools.uses WHERE request_id = $1")
            .bind(request_id.as_u64() as i64)
            .fetch_one(pool)
            .await?,
    )
}

#[tokio::test]
async fn a_sequential_session_is_set_up_once() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4().to_string();

    let first = captured(&fx, &session, Vec::new()).await;
    RequestStore::snapshot_terminal(&fx.store, first).await;
    let second = captured(&fx, &session, Vec::new()).await;
    RequestStore::snapshot_terminal(&fx.store, second).await;

    assert!(
        fx.store.read().await.session_is_new(&session),
        "the store forgets the session once its requests finish"
    );
    assert_eq!(fx.hook.0.load(Ordering::SeqCst), 1);
    fx.pool.close().await;
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn the_terminal_write_leaves_live_written_tool_uses_alone() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4().to_string();

    let id = captured(&fx, &session, vec![call("toolu_live", true)]).await;
    RequestStore::snapshot_terminal(&fx.store, id).await;

    assert_eq!(
        tool_use_count(&fx.pool, id).await?,
        0,
        "a completed call was written live, so the terminal write does not insert it again"
    );
    fx.pool.close().await;
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn the_terminal_write_keeps_tool_uses_the_stream_did_not_write() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4().to_string();

    let cut = captured(&fx, &session, vec![call("toolu_cut", false)]).await;
    RequestStore::snapshot_terminal(&fx.store, cut).await;
    let failed = captured(&fx, &session, vec![call("toolu_failed", true)]).await;
    fx.store.write().await.update(&failed, |req| {
        req.capture_writes.tool_use_failed = true;
    });
    RequestStore::snapshot_terminal(&fx.store, failed).await;

    assert_eq!(tool_use_count(&fx.pool, cut).await?, 1);
    assert_eq!(tool_use_count(&fx.pool, failed).await?, 1);
    fx.pool.close().await;
    fx.db.cleanup().await?;
    Ok(())
}
