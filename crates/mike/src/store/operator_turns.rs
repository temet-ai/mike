use crate::analysis::EmbeddingService;
use crate::store::persistence::{PersistError, PersistHandle};

pub(crate) use crate::store::persistence::operator_turns::{
    OperatorTurnRow, PendingChunk, PendingEmbeddingRow, TURN_SELECT_BASE,
};

pub(crate) use crate::store::persistence::operator_turns::vector_to_text_literal;

pub(crate) async fn embed_turn(
    handle: &PersistHandle,
    embedding_service: &EmbeddingService,
    turn_id: i64,
) -> Result<(), PersistError> {
    if !embedding_service.is_available() {
        return Ok(());
    }
    let Some(row) = handle.get_turn_for_embedding(turn_id).await? else {
        return Ok(());
    };
    let has_text =
        !row.user_prompt.trim().is_empty() || !row.final_assistant_text.trim().is_empty();
    let outcome = embed_and_write_chunks(handle, embedding_service, vec![row]).await?;
    if outcome.written > 0 || !has_text {
        handle.mark_turns_embedded(&[turn_id]).await?;
    }
    Ok(())
}

pub(crate) struct EmbedWriteOutcome {
    pub(crate) written: usize,
    pub(crate) call_failed: bool,
}

pub(crate) async fn embed_closed_pending(
    handle: &PersistHandle,
    embedding_service: &EmbeddingService,
) -> Result<EmbedWriteOutcome, PersistError> {
    if !embedding_service.is_available() {
        return Ok(EmbedWriteOutcome {
            written: 0,
            call_failed: false,
        });
    }
    const BATCH: i64 = 16;

    let ids = handle.fetch_unembedded_turn_ids(BATCH).await?;
    if ids.is_empty() {
        return Ok(EmbedWriteOutcome {
            written: 0,
            call_failed: false,
        });
    }

    let mut rows = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Some(row) = handle.get_turn_for_embedding(*id).await? {
            rows.push(row);
        }
    }
    if rows.is_empty() {
        return Ok(EmbedWriteOutcome {
            written: 0,
            call_failed: false,
        });
    }

    let turn_ids: Vec<i64> = rows.iter().map(|r| r.turn_id).collect();
    let outcome = embed_and_write_chunks(handle, embedding_service, rows).await?;
    if outcome.written > 0 {
        handle.mark_turns_embedded(&turn_ids).await?;
    }

    Ok(outcome)
}

impl EmbedWriteOutcome {
    fn no_work() -> Self {
        Self {
            written: 0,
            call_failed: false,
        }
    }
}

enum EmbedTextsOutcome {
    Vectors(Vec<Vec<f32>>),
    Failed,
}

fn build_chunk_plan(
    rows: &[PendingEmbeddingRow],
    max_chars: usize,
) -> (Vec<PendingChunk>, Vec<(i64, std::ops::Range<usize>)>) {
    let mut plan: Vec<PendingChunk> = Vec::new();
    let mut turn_ranges: Vec<(i64, std::ops::Range<usize>)> = Vec::with_capacity(rows.len());
    for row in rows {
        let start = plan.len();
        push_role_chunks(&mut plan, "prompt", &row.user_prompt, max_chars);
        push_role_chunks(&mut plan, "assistant", &row.final_assistant_text, max_chars);
        turn_ranges.push((row.turn_id, start..plan.len()));
    }
    (plan, turn_ranges)
}

fn push_role_chunks(
    plan: &mut Vec<PendingChunk>,
    chunk_role: &'static str,
    text: &str,
    max_chars: usize,
) {
    for (idx, chunk_text) in crate::analysis::embeddings::chunk_text(text, max_chars)
        .into_iter()
        .enumerate()
    {
        plan.push(PendingChunk {
            chunk_idx: idx.min(i32::MAX as usize) as i32,
            chunk_role,
            chunk_text,
        });
    }
}

async fn embed_plan_texts(
    embedding_service: &EmbeddingService,
    plan: &[PendingChunk],
    rows_len: usize,
) -> EmbedTextsOutcome {
    let texts: Vec<String> = plan.iter().map(|c| c.chunk_text.clone()).collect();
    let vectors = match embedding_service.embed_documents(&texts).await {
        Ok(v) => v,
        Err(e) => {
            warn_embed_documents_failed(&e, plan.len(), rows_len);
            return EmbedTextsOutcome::Failed;
        }
    };
    if vectors.len() != plan.len() {
        warn_embed_batch_mismatched_count(vectors.len(), plan.len());
        return EmbedTextsOutcome::Failed;
    }
    EmbedTextsOutcome::Vectors(vectors)
}

fn warn_embed_documents_failed(
    error: &crate::analysis::embeddings::EmbedError,
    chunks: usize,
    rows: usize,
) {
    tracing::warn!(
        error = %error,
        chunks,
        rows,
        "operator_turn_chunks embed_documents failed; rows left for next backfill"
    );
}

fn warn_embed_batch_mismatched_count(got: usize, expected: usize) {
    tracing::warn!(
        got,
        expected,
        "operator_turn_chunks embed_batch returned mismatched count; skipping write"
    );
}

async fn write_turn_chunks(
    handle: &PersistHandle,
    plan: &[PendingChunk],
    vectors: &[Vec<f32>],
    turn_ranges: Vec<(i64, std::ops::Range<usize>)>,
    model_name: &str,
) -> EmbedWriteOutcome {
    let mut written_turns = 0usize;
    let mut attempted_turns = 0usize;
    for (turn_id, range) in turn_ranges {
        let chunk_count = range.end - range.start;
        if chunk_count == 0 {
            continue;
        }
        attempted_turns += 1;
        match handle
            .replace_turn_chunks(turn_id, &plan[range.clone()], &vectors[range], model_name)
            .await
        {
            Ok(()) => {
                written_turns = written_turns.saturating_add(1);
            }
            Err(e) => {
                tracing::warn!(
                    turn_id = turn_id,
                    error = %e,
                    "operator_turn_chunks insert failed; row left for next backfill"
                );
            }
        }
    }
    EmbedWriteOutcome {
        written: written_turns,
        call_failed: attempted_turns > 0 && written_turns == 0,
    }
}

async fn embed_and_write_chunks(
    handle: &PersistHandle,
    embedding_service: &EmbeddingService,
    rows: Vec<PendingEmbeddingRow>,
) -> Result<EmbedWriteOutcome, PersistError> {
    if rows.is_empty() {
        return Ok(EmbedWriteOutcome::no_work());
    }
    let max_chars = embedding_service.chunk_max_chars();
    let (plan, turn_ranges) = build_chunk_plan(&rows, max_chars);
    if plan.is_empty() {
        return Ok(EmbedWriteOutcome::no_work());
    }
    let model_name = embedding_service.model_name();
    let vectors = match embed_plan_texts(embedding_service, &plan, rows.len()).await {
        EmbedTextsOutcome::Vectors(v) => v,
        EmbedTextsOutcome::Failed => {
            return Ok(EmbedWriteOutcome {
                written: 0,
                call_failed: true,
            });
        }
    };
    if embedding_service.model_name() != model_name {
        return Ok(EmbedWriteOutcome::no_work());
    }
    Ok(write_turn_chunks(handle, &plan, &vectors, turn_ranges, &model_name).await)
}

pub struct PromptKindClassificationInput<'a> {
    pub turn_origin: &'a crate::extract::request::TurnOrigin,
    pub turn_type: &'a crate::extract::request::TurnType,
    pub request_id: crate::store::models::RequestId,
    pub session_id: &'a str,
    pub session_db_id: Option<i64>,
    pub human_prompt: &'a str,
    pub last_user_message: &'a str,
    pub author: &'a str,
    pub has_agent_id: bool,
}

pub async fn record_prompt_kind_classification(
    persist: Option<&PersistHandle>,
    input: PromptKindClassificationInput<'_>,
) {
    use crate::extract::request::{TurnOrigin, TurnType};
    use crate::wire::genuine::repeat::repeat_key;
    use crate::wire::genuine::{PromptInput, classify_prompt};

    let is_operator_turn_opener = matches!(
        input.turn_origin,
        TurnOrigin::Primary | TurnOrigin::PrimaryFirst
    ) && *input.turn_type == TurnType::UserPrompt;
    if !is_operator_turn_opener {
        return;
    }
    let Some(persist) = persist else {
        return;
    };
    let Some(db_id) = input.session_db_id else {
        return;
    };
    let class = classify_prompt(&PromptInput {
        text: input.human_prompt,
        last_user_message: Some(input.last_user_message),
        author: input.author,
        has_agent_id: input.has_agent_id,
    });
    let key = repeat_key(input.human_prompt);
    if let Err(e) = persist
        .write_prompt_classification(
            input.request_id.as_u64() as i64,
            db_id,
            &class,
            key.as_deref(),
        )
        .await
    {
        tracing::warn!(
            request_id = %input.request_id,
            session_id = %input.session_id,
            error = %e,
            "prompt_kind classification write failed (non-fatal)"
        );
    }
}

#[cfg(test)]
mod tests;

/// Whether a turn's outcome predates projection (legacy) or awaits it (pending).
pub(crate) fn projection_flags(outcome_source: Option<&str>, outcome: &str) -> (bool, bool) {
    let unprojected = outcome_source.is_none();
    (
        unprojected && outcome != "unknown",
        unprojected && outcome == "unknown",
    )
}
