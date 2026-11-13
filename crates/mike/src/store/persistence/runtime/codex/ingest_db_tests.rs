use crate::store::persistence::runtime_repo::RuntimePersist as _;
use chrono::Utc;
use uuid::Uuid;

use super::{CodexCaptureOutcome, CodexItemPage};
use crate::runtime::codex::ingest::transcript::{Direction, LIVE_TURN, RECONNECT, Transcript};
use crate::runtime::codex::ingest::{IngestConnection, IngestOutcome};
use crate::runtime::codex::{Field, ProjectionStatus, TextChannel};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::runtime::tests::database;

async fn capture_all(
    handle: &PersistHandle,
    transcript: &Transcript,
    connection: &mut IngestConnection,
) -> anyhow::Result<usize> {
    let mut captured = 0;
    for (direction, frame) in &transcript.frames {
        if *direction == Direction::Sent {
            connection.sent(frame.as_bytes());
            continue;
        }
        let IngestOutcome::Capture(frame) =
            connection.received(frame.as_bytes().to_vec(), Utc::now())?
        else {
            continue;
        };
        match handle.runtime().capture_codex_frame(&frame).await? {
            CodexCaptureOutcome::Captured { .. } => captured += 1,
            CodexCaptureOutcome::Excluded(reason) => anyhow::bail!("excluded: {reason:?}"),
        }
    }
    Ok(captured)
}

#[tokio::test]
async fn recorded_session_and_reconnect_project_into_one_item_per_provider_id() -> anyhow::Result<()>
{
    let Some((_db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let live = Transcript::parse(LIVE_TURN)?;
    let mut connection = IngestConnection::new(namespace.id, live.source()?);
    let first = capture_all(&handle, &live, &mut connection).await?;
    connection.reconnect();
    let second = capture_all(&handle, &Transcript::parse(RECONNECT)?, &mut connection).await?;
    let generation = handle
        .runtime()
        .begin_codex_projection_generation(namespace.id, Uuid::new_v4())
        .await?;
    let applied = handle
        .runtime()
        .drain_codex_projection(generation.generation_id)
        .await?;
    assert_eq!(applied.len(), first + second);
    assert!(
        applied
            .iter()
            .all(|application| application.status != ProjectionStatus::Malformed)
    );
    let thread = live.header_text("thread_id")?;
    let items = handle
        .runtime()
        .list_codex_items(CodexItemPage {
            generation_id: generation.generation_id,
            thread_id: thread,
            after: None,
            limit: 500,
        })
        .await?;
    let mut ids: Vec<_> = items
        .iter()
        .map(|stored| stored.item.key.item_id.clone())
        .collect();
    let count = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), count);
    assert_eq!(count, 13);
    let command = items
        .iter()
        .find(|stored| {
            stored
                .item
                .streamed_text
                .contains_key(&TextChannel::CommandOutput)
        })
        .ok_or_else(|| anyhow::anyhow!("no command output stored"))?;
    assert!(command.item.terminal);
    assert!(command.latest_capture_sequence > command.first_capture_sequence);
    let interrupted = items
        .iter()
        .find(|stored| stored.item.key.item_id.starts_with("exec-71ce8f25"))
        .and_then(|stored| stored.item.snapshot.as_ref())
        .ok_or_else(|| anyhow::anyhow!("interrupted command missing"))?;
    assert_eq!(interrupted.status, Field::Value("failed".into()));
    assert!(items.iter().all(|stored| stored.item.terminal));
    Ok(())
}
