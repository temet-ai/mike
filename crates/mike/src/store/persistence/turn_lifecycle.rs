#[cfg(test)]
mod aggregate_tests;
mod inheritance;
#[cfg(test)]
mod text_tests;

pub(crate) use inheritance::{
    attribute_inherited_requests, inherited_request_turn, reconcile_dispatch_results,
};

use super::PersistError;
use super::capture_classification::classify_author_kind;
use crate::store::models::InterceptedRequest;
use crate::wire::genuine::disposition::{LaneFacts, TurnDisposition, turn_disposition};
use crate::wire::genuine::{PromptInput, classify_prompt};

pub(crate) async fn attribute_turn_subject(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    turn_id: i64,
) -> Result<(), PersistError> {
    sqlx::query_file!(
        "src/store/persistence/turn_lifecycle/subject_attribution.sql",
        turn_id
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) struct TurnLifecycleCtx {
    pub(crate) session_db_id: i64,
    pub(crate) project_id: i64,
    pub(crate) request_db_id: i64,
    pub(crate) disposition: TurnDisposition,
    pub(crate) human_prompt: String,
    pub(crate) is_streaming: bool,
}

pub(crate) fn opened_by_code(disposition: TurnDisposition) -> Option<&'static str> {
    match disposition {
        TurnDisposition::OperatorTurn => Some("operator"),
        TurnDisposition::MachineSegment => Some("machine"),
        TurnDisposition::Attach | TurnDisposition::Ignore => None,
    }
}

pub(crate) fn disposition_for_request(req: &InterceptedRequest) -> TurnDisposition {
    let has_agent_id = req.claude_code_agent_id.is_some();
    let author = classify_author_kind(
        &req.turn_origin,
        &req.human_prompt,
        &req.last_user_message,
        req.is_tool_loop,
        has_agent_id,
    );
    let class = classify_prompt(&PromptInput {
        text: &req.human_prompt,
        last_user_message: Some(&req.last_user_message),
        author,
        has_agent_id,
    });
    turn_disposition(
        &LaneFacts {
            turn_origin: &req.turn_origin,
            is_user_prompt_type: req.turn_type.eq_ignore_ascii_case("user_prompt"),
            has_agent_id,
        },
        &class,
    )
}

pub(crate) async fn materialize_turn_aggregates(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    turn_id: i64,
) -> Result<(), PersistError> {
    sqlx::query_file!(
        "src/store/persistence/turn_lifecycle/materialize.sql",
        turn_id
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn close_predecessor_turn_and_materialize(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    prev_id: i64,
    request_db_id: i64,
) -> Result<(), PersistError> {
    sqlx::query!(
        "UPDATE memory.operator_turns t
                SET closed_at                   = NOW(),
                    outcome_evidence_request_id = $2,
                    classified_at               = NOW()
              WHERE t.id = $1",
        prev_id,
        request_db_id,
    )
    .execute(&mut **tx)
    .await?;
    materialize_turn_aggregates(tx, prev_id).await
}

async fn open_new_turn(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ctx: &TurnLifecycleCtx,
    opened_by: &str,
) -> Result<Option<i64>, PersistError> {
    let new_turn_id = sqlx::query_scalar!(
        "INSERT INTO memory.operator_turns (
            session_id, project_id,
            start_request_id,
            end_request_id,
            user_prompt,
            opened_by
         )
         SELECT $1, $2, $3, $3, $4, $5
         WHERE NOT EXISTS (
             SELECT 1 FROM memory.operator_turns
             WHERE session_id = $1 AND closed_at IS NULL
         )
         RETURNING id",
        ctx.session_db_id,
        ctx.project_id,
        ctx.request_db_id,
        &ctx.human_prompt,
        opened_by,
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(new_turn_id)
}

async fn stamp_turn_membership(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    turn_id: i64,
    request_db_id: i64,
) -> Result<(), PersistError> {
    sqlx::query!(
        "UPDATE requests.requests SET operator_turn_id = $1 WHERE id = $2",
        turn_id,
        request_db_id,
    )
    .execute(&mut **tx)
    .await?;
    attribute_turn_subject(tx, turn_id).await?;
    Ok(())
}

async fn extend_open_turn(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    prev_id: i64,
    request_db_id: i64,
) -> Result<(), PersistError> {
    sqlx::query!(
        "UPDATE memory.operator_turns
            SET end_request_id         = $2
          WHERE id = $1",
        prev_id,
        request_db_id,
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "UPDATE requests.requests SET operator_turn_id = $1 WHERE id = $2",
        prev_id,
        request_db_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn write_nonstreaming_text_block(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ctx: &TurnLifecycleCtx,
    response_body: &[u8],
) -> Result<(), PersistError> {
    let text = crate::extract::response::extract_accumulated_text_from_body(response_body);
    if text.is_empty() {
        return Ok(());
    }
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(text.as_bytes());
    let content_sha256 = hex::encode(hasher.finalize());
    let byte_size = text.len() as i32;
    sqlx::query!(
        "INSERT INTO responses.stream_blocks
             (request_id, session_id, block_type_id, seq_ord,
              content, signature, wire_tool_use_id,
              content_sha256, byte_size)
             SELECT $1, $2, sbt.id, $3, $4, NULL, NULL, $5, $6
             FROM responses.stream_block_types sbt
             WHERE sbt.code = $7
             ON CONFLICT (request_id, seq_ord) DO NOTHING",
        ctx.request_db_id,
        ctx.session_db_id,
        0_i32,
        &text,
        &content_sha256,
        byte_size,
        "text",
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn write_nonstreaming_response(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ctx: &TurnLifecycleCtx,
    response_body: Option<&[u8]>,
) -> Result<(), PersistError> {
    if !ctx.is_streaming
        && let Some(body) = response_body
    {
        write_nonstreaming_text_block(tx, ctx, body).await?;
    }
    Ok(())
}

pub(crate) async fn apply_operator_turn_lifecycle(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ctx: &TurnLifecycleCtx,
    response_body: Option<&[u8]>,
) -> Result<Option<i64>, PersistError> {
    attribute_inherited_requests(tx, ctx.request_db_id).await?;
    write_nonstreaming_response(tx, ctx, response_body).await?;
    if let Some(turn_id) = inherited_request_turn(tx, ctx.request_db_id).await? {
        materialize_turn_aggregates(tx, turn_id).await?;
        return Ok(None);
    }
    let open_turn_id = sqlx::query_scalar!(
        "SELECT id FROM memory.operator_turns
          WHERE session_id = $1 AND closed_at IS NULL
          FOR UPDATE",
        ctx.session_db_id,
    )
    .fetch_optional(&mut **tx)
    .await?;

    let mut closed_turn_id = None;
    match ctx.disposition {
        TurnDisposition::OperatorTurn | TurnDisposition::MachineSegment => {
            if let Some(prev_id) = open_turn_id {
                close_predecessor_turn_and_materialize(tx, prev_id, ctx.request_db_id).await?;
                closed_turn_id = Some(prev_id);
            }
            if let Some(opened_by) = opened_by_code(ctx.disposition)
                && let Some(turn_id) = open_new_turn(tx, ctx, opened_by).await?
            {
                stamp_turn_membership(tx, turn_id, ctx.request_db_id).await?;
            }
        }
        TurnDisposition::Attach => {
            if let Some(prev_id) = open_turn_id {
                extend_open_turn(tx, prev_id, ctx.request_db_id).await?;
            }
        }
        TurnDisposition::Ignore => {}
    }

    Ok(closed_turn_id)
}
