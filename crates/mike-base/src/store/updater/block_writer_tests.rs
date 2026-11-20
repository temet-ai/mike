use std::sync::Arc;

use tokio::sync::RwLock;

use super::{BlockAccumulator, BlockKind, BlockWriteTarget};
use crate::store::RequestStore;
use crate::store::models::RequestId;
use crate::store::retention::RetentionPolicy;
use crate::store::tests::make_test_request;

fn store_with(id: RequestId) -> Arc<RwLock<RequestStore>> {
    let mut store = RequestStore::new(RetentionPolicy::default());
    store.insert(make_test_request(Some(id), "sess-fallback-blocks"));
    Arc::new(RwLock::new(store))
}

#[tokio::test]
async fn stream_blocks_on_fallback_id_buffered_for_terminal_retry() -> anyhow::Result<()> {
    let id = RequestId::fallback();
    let store = store_with(id);
    let target = BlockWriteTarget {
        persist: &None,
        store: &store,
        request_id: id,
    };
    let mut blocks = BlockAccumulator::default();
    blocks.switch_to(BlockKind::Text, &target).await;
    blocks.push_text("hello");
    blocks.switch_to(BlockKind::ToolUse, &target).await;
    blocks.write_tool_use_block(&target, "toolu_fallback").await;

    let s = store.read().await;
    let Some(req) = s.get(&id) else {
        anyhow::bail!("request {id} missing from store");
    };
    let buffered: Vec<(i32, &str, &str, Option<&str>)> = req
        .failed_stream_blocks
        .iter()
        .map(|b| {
            (
                b.seq_ord,
                b.block_type.as_str(),
                b.content.as_str(),
                b.wire_tool_use_id.as_deref(),
            )
        })
        .collect();
    anyhow::ensure!(
        buffered
            == vec![
                (0, "text", "hello", None),
                (1, "tool_use", "", Some("toolu_fallback")),
            ],
        "unexpected buffered blocks: {buffered:?}"
    );
    anyhow::ensure!(blocks.next_seq_ord == 2);
    Ok(())
}

#[tokio::test]
async fn stream_blocks_on_real_id_without_persist_not_buffered() -> anyhow::Result<()> {
    let id = RequestId::next();
    let store = store_with(id);
    let target = BlockWriteTarget {
        persist: &None,
        store: &store,
        request_id: id,
    };
    let mut blocks = BlockAccumulator::default();
    blocks.switch_to(BlockKind::Text, &target).await;
    blocks.push_text("hello");
    blocks.flush(&target).await;

    let s = store.read().await;
    let Some(req) = s.get(&id) else {
        anyhow::bail!("request {id} missing from store");
    };
    anyhow::ensure!(req.failed_stream_blocks.is_empty());
    Ok(())
}
