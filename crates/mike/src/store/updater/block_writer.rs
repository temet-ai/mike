use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use crate::store::RequestStore;
use crate::store::models::{FailedStreamBlock, RequestId};
use crate::store::persistence::{PersistHandle, StreamBlockRow};

pub(super) async fn write_tool_use_row(
    persist: &Option<PersistHandle>,
    request_id: i64,
    request_created_at: chrono::DateTime<chrono::Utc>,
    ord: i32,
    tc: &crate::types::ToolCall,
) {
    let Some(p) = persist.as_ref() else {
        return;
    };
    if let Err(e) = p
        .insert_tool_use_row(request_id, request_created_at, ord, tc)
        .await
    {
        tracing::warn!(
            tool_id = %tc.tool_id,
            error = %e,
            "tool_use DB insert failed; Phase 11 crash-recovery will retry at terminal"
        );
    }
}

pub(super) const DEFAULT_SUBAGENT_TYPE: &str = "general-purpose";

pub(super) fn extract_subagent_type(input_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(input_json)
        .ok()?
        .get("subagent_type")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(super) async fn write_agent_dispatch_row(
    persist: &Option<PersistHandle>,
    tool_use_id: &str,
    agent_name: &str,
) {
    let Some(p) = persist.as_ref() else {
        return;
    };
    if let Err(e) = p.insert_agent_dispatch_row(tool_use_id, agent_name).await {
        tracing::warn!(
            tool_use_id = %tool_use_id,
            error = %e,
            "agent_dispatch DB insert failed; Phase 11 crash-recovery will retry at terminal"
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BlockKind {
    Text,
    Thinking,
    ToolUse,
}

impl BlockKind {
    fn code(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Thinking => "thinking",
            Self::ToolUse => "tool_use",
        }
    }
}

#[derive(Default)]
pub(super) struct BlockAccumulator {
    kind: Option<BlockKind>,
    content: String,
    signature: Option<String>,
    pub(super) next_seq_ord: i32,
}

pub(super) struct BlockWriteTarget<'a> {
    pub(super) persist: &'a Option<PersistHandle>,
    pub(super) store: &'a Arc<RwLock<RequestStore>>,
    pub(super) request_id: RequestId,
}

impl BlockAccumulator {
    pub(super) async fn switch_to(&mut self, kind: BlockKind, target: &BlockWriteTarget<'_>) {
        if self.kind == Some(kind) {
            return;
        }
        self.flush(target).await;
        self.kind = Some(kind);
    }

    pub(super) fn push_text(&mut self, text: &str) {
        self.content.push_str(text);
    }

    pub(super) fn set_signature(&mut self, signature: String) {
        self.signature = Some(signature);
    }

    pub(super) async fn flush(&mut self, target: &BlockWriteTarget<'_>) {
        let Some(kind) = self.kind.take() else {
            self.reset_block_state();
            return;
        };
        if kind == BlockKind::ToolUse {
            self.reset_block_state();
            return;
        }
        let content = std::mem::take(&mut self.content);
        let signature = self.signature.take();
        let seq_ord = self.next_seq_ord;
        self.next_seq_ord = self.next_seq_ord.saturating_add(1);
        write_stream_block_row(
            target,
            seq_ord,
            kind.code(),
            &content,
            signature.as_deref(),
            None,
        )
        .await;
    }

    pub(super) async fn write_tool_use_block(
        &mut self,
        target: &BlockWriteTarget<'_>,
        wire_tool_use_id: &str,
    ) {
        self.flush(target).await;
        let seq_ord = self.next_seq_ord;
        self.next_seq_ord = self.next_seq_ord.saturating_add(1);
        write_stream_block_row(
            target,
            seq_ord,
            BlockKind::ToolUse.code(),
            "",
            None,
            Some(wire_tool_use_id),
        )
        .await;
    }

    fn reset_block_state(&mut self) {
        self.content.clear();
        self.signature = None;
    }
}

struct BlockToWrite<'a> {
    request_id: i64,
    seq_ord: i32,
    block_type: &'a str,
    content: &'a str,
    signature: Option<&'a str>,
    wire_tool_use_id: Option<&'a str>,
}

async fn write_stream_block_row(
    target: &BlockWriteTarget<'_>,
    seq_ord: i32,
    block_type: &str,
    content: &str,
    signature: Option<&str>,
    wire_tool_use_id: Option<&str>,
) {
    let block = BlockToWrite {
        request_id: i64::try_from(target.request_id.as_u64()).unwrap_or(i64::MAX),
        seq_ord,
        block_type,
        content,
        signature,
        wire_tool_use_id,
    };
    persist_block(target, &block).await;
}

async fn persist_block(target: &BlockWriteTarget<'_>, block: &BlockToWrite<'_>) {
    let Some(handle) = target.persist.as_ref() else {
        return;
    };
    let mut hasher = Sha256::new();
    hasher.update(block.content.as_bytes());
    let content_sha256 = hex::encode(hasher.finalize());
    if let Err(e) = handle
        .insert_stream_block_row(StreamBlockRow {
            request_id: block.request_id,
            seq_ord: block.seq_ord,
            block_type: block.block_type,
            content: block.content,
            signature: block.signature,
            wire_tool_use_id: block.wire_tool_use_id,
            content_sha256: &content_sha256,
        })
        .await
    {
        tracing::warn!(
            request_id = block.request_id,
            seq_ord = block.seq_ord,
            block_type = block.block_type,
            error = %e,
            "stream block DB insert failed; buffered for terminal ENSURE retry",
        );
        let failed = FailedStreamBlock {
            seq_ord: block.seq_ord,
            block_type: block.block_type.to_string(),
            content: block.content.to_string(),
            signature: block.signature.map(str::to_string),
            wire_tool_use_id: block.wire_tool_use_id.map(str::to_string),
            content_sha256,
        };
        let mut s = target.store.write().await;
        s.update(&target.request_id, |req| {
            req.failed_stream_blocks.push(failed);
        });
    }
}
