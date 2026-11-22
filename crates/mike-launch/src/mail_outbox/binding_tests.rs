use std::time::Duration;

use sqlx::postgres::PgListener;
use uuid::Uuid;

use super::mailbox_link::codex_bound;
use super::scripted::database;
use crate::launch::codex::bind_seat_thread;

#[tokio::test]
async fn binding_a_codex_thread_notifies_the_feed_and_counts_as_bound() -> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        return Ok(());
    };
    assert!(!codex_bound(persist.pool()).await?);
    let mut listener = PgListener::connect(&db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    let session = Uuid::new_v4();
    let home = std::env::temp_dir().join(format!("mike-outbox-binding-{session}"));
    bind_seat_thread(&persist, &home, session, "thread-bound", None).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut notified = false;
    while let Ok(Ok(note)) = tokio::time::timeout_at(deadline, listener.recv()).await {
        let payload: serde_json::Value = serde_json::from_str(note.payload())?;
        if payload["table"] == "runtime_threads" {
            assert_eq!(payload["session_id"], session.to_string());
            notified = true;
            break;
        }
    }
    assert!(notified, "binding a Codex thread sent no runtime_threads notice");
    assert!(codex_bound(persist.pool()).await?);
    db.cleanup().await
}
