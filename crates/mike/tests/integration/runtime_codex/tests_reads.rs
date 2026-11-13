use mike::store::persistence::runtime_repo::RuntimePersist as _;
use serde_json::json;
use uuid::Uuid;

use mike::runtime::RuntimeObservation;
use mike::runtime::codex::TextChannel;

use super::RawEventPage;
use super::test_support::{captured, database, delta, input, key, receipt_id};

#[tokio::test]
async fn raw_receipt_pagination_returns_whole_trusted_and_legacy_receipts() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let first = captured(
        handle
            .runtime()
            .capture_codex_receipt(input(namespace.id, delta("first\0text"))?)
            .await?,
    )?;
    let legacy_input = input(namespace.id, json!({"unknown":"legacy\0data"}))?;
    let legacy = RuntimeObservation::new(
        legacy_input.envelope.clone(),
        legacy_input.raw_payload.clone(),
    )?;
    handle.runtime().append_runtime_observation(&legacy).await?;
    let legacy = handle
        .runtime()
        .runtime_receipt(receipt_id(&legacy_input))
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing legacy receipt"))?;
    let last = captured(
        handle
            .runtime()
            .capture_codex_receipt(input(namespace.id, delta("last\0text"))?)
            .await?,
    )?;
    let expected = vec![first, legacy.clone(), last];
    let mut cursor = 0;
    let mut actual = Vec::new();
    for expected_row in &expected {
        let rows = handle
            .runtime()
            .list_runtime_raw_events(RawEventPage {
                namespace_id: namespace.id,
                after_event_id: cursor,
                limit: 1,
            })
            .await?;
        assert_eq!(rows.as_slice(), std::slice::from_ref(expected_row));
        cursor = expected_row.event_id;
        actual.extend(rows);
    }
    assert_eq!(actual, expected);
    assert!(
        handle
            .runtime()
            .list_runtime_raw_events(RawEventPage {
                namespace_id: namespace.id,
                after_event_id: cursor,
                limit: 1
            })
            .await?
            .is_empty()
    );
    assert_eq!(
        handle
            .runtime()
            .list_codex_unclassified_receipts(RawEventPage {
                namespace_id: namespace.id,
                after_event_id: 0,
                limit: 1
            })
            .await?,
        vec![legacy]
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn terminal_command_state_survives_reload_and_keeps_late_output_as_evidence()
-> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let generation_id = Uuid::new_v4();
    handle
        .runtime()
        .begin_codex_projection_generation(namespace.id, generation_id)
        .await?;
    let frames = [
        json!({"method":"item/started","params":{"threadId":"thread","turnId":"turn","item":{
            "id":"item","type":"commandExecution","command":"fixture\0command","status":"inProgress"
        }}}),
        json!({"method":"item/commandExecution/outputDelta","params":{
            "threadId":"thread","turnId":"turn","itemId":"item","delta":"stream\0"
        }}),
        json!({"method":"item/completed","params":{"threadId":"thread","turnId":"turn","item":{
            "id":"item","type":"commandExecution","status":"completed","aggregatedOutput":"complete\0output","exitCode":0
        }}}),
        json!({"method":"item/commandExecution/outputDelta","params":{
            "threadId":"thread","turnId":"turn","itemId":"item","delta":"late"
        }}),
    ];
    for (index, frame) in frames.into_iter().enumerate() {
        handle
            .runtime()
            .capture_codex_receipt(input(namespace.id, frame)?)
            .await?;
        handle
            .runtime()
            .apply_next_codex_projection(generation_id)
            .await?;
        let restored = handle
            .runtime()
            .codex_projected_item(generation_id, &key(namespace.id))
            .await?
            .ok_or_else(|| anyhow::anyhow!("missing command"))?;
        assert_eq!(restored.item.terminal, index >= 2);
    }
    let state = handle
        .runtime()
        .codex_projected_item(generation_id, &key(namespace.id))
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing terminal command"))?;
    assert!(state.item.terminal);
    assert!(state.item.reconciliation_required);
    assert_eq!(state.first_capture_sequence, 1);
    assert_eq!(state.latest_capture_sequence, 4);
    assert_eq!(
        state
            .item
            .display_text
            .get(&TextChannel::CommandOutput)
            .map(String::as_str),
        Some("complete\0output")
    );
    assert_eq!(
        state
            .item
            .streamed_text
            .get(&TextChannel::CommandOutput)
            .map(String::as_str),
        Some("stream\0late")
    );
    let snapshot = state
        .item
        .snapshot
        .ok_or_else(|| anyhow::anyhow!("missing snapshot"))?;
    let command = snapshot
        .command
        .ok_or_else(|| anyhow::anyhow!("missing command facts"))?;
    assert_eq!(
        command.command.value().map(String::as_str),
        Some("fixture\0command")
    );
    assert_eq!(command.exit_code.value(), Some(&0));
    assert_eq!(
        handle
            .runtime()
            .list_codex_projection_effects(super::test_support::page(generation_id))
            .await?
            .len(),
        4
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
