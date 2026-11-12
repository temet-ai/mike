use super::turn_lifecycle;
use super::{PersistError, PersistHandle};
use crate::wire::genuine::disposition::{LaneFacts, TurnDisposition, turn_disposition};
use crate::wire::genuine::{PromptInput, classify_prompt};
use chrono::Utc;

struct AbortedRequestFacts {
    session_id: i64,
    turn_origin: String,
    turn_type: String,
    has_agent_id: bool,
    human_prompt: String,
    last_user_message: String,
    author: String,
}

impl AbortedRequestFacts {
    fn disposition(&self) -> TurnDisposition {
        let class = classify_prompt(&PromptInput {
            text: &self.human_prompt,
            last_user_message: Some(&self.last_user_message),
            author: &self.author,
            has_agent_id: self.has_agent_id,
        });
        turn_disposition(
            &LaneFacts {
                turn_origin: &self.turn_origin,
                is_user_prompt_type: self.turn_type == "user_prompt",
                has_agent_id: self.has_agent_id,
            },
            &class,
        )
    }

    fn is_main_lane_without_agent(&self) -> bool {
        !self.has_agent_id && matches!(self.turn_origin.as_str(), "primary" | "primary_first")
    }
}

async fn load_aborted_request_facts(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
) -> Result<AbortedRequestFacts, PersistError> {
    let facts = sqlx::query_as!(
        AbortedRequestFacts,
        r#"SELECT r.session_id,
                ro.code AS turn_origin,
                tt.code AS turn_type,
                r.claude_code_agent_id IS NOT NULL AS "has_agent_id!",
                r.human_prompt,
                r.last_user_message,
                ak.code AS author
           FROM requests.requests r
           JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
           JOIN requests.turn_types tt ON tt.id = r.turn_type_id
           JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
          WHERE r.id = $1"#,
        id,
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(facts)
}

async fn find_open_turn_for_session(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session_db_id: i64,
) -> Result<Option<i64>, PersistError> {
    let open_turn_id = sqlx::query_scalar!(
        "SELECT id FROM memory.operator_turns
          WHERE session_id = $1 AND closed_at IS NULL",
        session_db_id,
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(open_turn_id)
}

async fn flip_pending_to_error(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
) -> Result<bool, PersistError> {
    let flipped: Option<i64> = sqlx::query_scalar!(
        "WITH upd AS (
             UPDATE requests.requests r
                SET status_id = error_s.id,
                    completed_at = NOW()
               FROM requests.statuses error_s,
                    requests.statuses pending_s
              WHERE error_s.code = 'error'
                AND pending_s.code = 'pending'
                AND r.id = $1
                AND r.status_id = pending_s.id
              RETURNING r.id
         )
         INSERT INTO requests.errors (request_id, message)
         SELECT id, 'stream aborted' FROM upd
         RETURNING request_id",
        id,
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(flipped.is_some())
}

async fn close_open_turn_for_session(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session_db_id: i64,
) -> Result<Option<i64>, PersistError> {
    let aborted_turn_id: Option<i64> = sqlx::query_scalar!(
        "UPDATE memory.operator_turns t
     SET closed_at = NOW()
    WHERE t.session_id = $1
      AND t.closed_at  IS NULL
    RETURNING t.id",
        session_db_id,
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(aborted_turn_id)
}

async fn open_replacement_turn(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
    opened_by: &str,
) -> Result<Option<i64>, PersistError> {
    let replacement_turn_id = sqlx::query_scalar!(
        "INSERT INTO memory.operator_turns (
            session_id, project_id,
            start_request_id, end_request_id,
            user_prompt, opened_by
         )
         SELECT r.session_id, r.project_id, r.id, r.id, r.human_prompt, $2
           FROM requests.requests r
          WHERE r.id = $1
            AND NOT EXISTS (
                SELECT 1 FROM memory.operator_turns
                WHERE session_id = r.session_id AND closed_at IS NULL
            )
         RETURNING id",
        id,
        opened_by,
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(replacement_turn_id)
}

async fn stamp_turn_membership_on_abort(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
    replacement_turn_id: Option<i64>,
    aborted_turn_id: Option<i64>,
) -> Result<(), PersistError> {
    if let Some(turn_id) = replacement_turn_id {
        sqlx::query!(
            "UPDATE requests.requests SET operator_turn_id = $1 WHERE id = $2",
            turn_id,
            id,
        )
        .execute(&mut **tx)
        .await?;
        turn_lifecycle::attribute_turn_subject(tx, turn_id).await?;
    } else if let Some(turn_id) = aborted_turn_id {
        sqlx::query!(
            "UPDATE requests.requests SET operator_turn_id = $1 WHERE id = $2",
            turn_id,
            id,
        )
        .execute(&mut **tx)
        .await?;
        sqlx::query!(
            "UPDATE memory.operator_turns
                SET end_request_id = GREATEST(end_request_id, $2)
              WHERE id = $1",
            turn_id,
            id,
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn reconcile_session_last_status(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session_db_id: i64,
) -> Result<(), PersistError> {
    sqlx::query!(
        "UPDATE sessions.sessions
     SET last_seen = GREATEST(last_seen, NOW()),
         last_status = 'Error: stream aborted'
     WHERE id = $1",
        session_db_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn apply_abort_disposition(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
    facts: &AbortedRequestFacts,
) -> Result<(), PersistError> {
    let session_db_id = facts.session_id;
    let disposition = facts.disposition();
    match disposition {
        TurnDisposition::OperatorTurn | TurnDisposition::MachineSegment => {
            let aborted_turn_id = close_open_turn_for_session(tx, session_db_id).await?;
            let replacement_turn_id = match turn_lifecycle::opened_by_code(disposition) {
                Some(opened_by) => open_replacement_turn(tx, id, opened_by).await?,
                None => None,
            };
            stamp_turn_membership_on_abort(tx, id, replacement_turn_id, aborted_turn_id).await?;
            if let Some(turn_id) = aborted_turn_id {
                turn_lifecycle::materialize_turn_aggregates(tx, turn_id).await?;
            }
        }
        TurnDisposition::Attach if facts.is_main_lane_without_agent() => {
            let aborted_turn_id = close_open_turn_for_session(tx, session_db_id).await?;
            stamp_turn_membership_on_abort(tx, id, None, aborted_turn_id).await?;
            if let Some(turn_id) = aborted_turn_id {
                turn_lifecycle::materialize_turn_aggregates(tx, turn_id).await?;
            }
        }
        TurnDisposition::Attach => {
            let open_turn_id = find_open_turn_for_session(tx, session_db_id).await?;
            stamp_turn_membership_on_abort(tx, id, None, open_turn_id).await?;
        }
        TurnDisposition::Ignore => {}
    }
    Ok(())
}

impl PersistHandle {
    pub(crate) async fn close_aborted_request(&self, request_id: u64) -> Result<(), PersistError> {
        crate::metrics::time_db("close_aborted_request", async move {
            let id = request_id as i64;
            let mut tx = self.pool.begin().await?;

            let facts = load_aborted_request_facts(&mut tx, id).await?;
            let session_db_id = facts.session_id;

            if !flip_pending_to_error(&mut tx, id).await? {
                tx.commit().await?;
                tracing::debug!(
                    request_id = id,
                    session_db_id,
                    "abort signal for already-terminal request — no-op"
                );
                return Ok(());
            }

            turn_lifecycle::attribute_inherited_requests(&mut tx, id).await?;
            if let Some(turn_id) = turn_lifecycle::inherited_request_turn(&mut tx, id).await? {
                turn_lifecycle::materialize_turn_aggregates(&mut tx, turn_id).await?;
                reconcile_session_last_status(&mut tx, session_db_id).await?;
                tx.commit().await?;
                return Ok(());
            }

            apply_abort_disposition(&mut tx, id, &facts).await?;

            reconcile_session_last_status(&mut tx, session_db_id).await?;

            tx.commit().await?;

            tracing::debug!(request_id = id, session_db_id, "closed aborted request");
            Ok(())
        })
        .await
    }

    const CRASH_RECOVERY_MIN_AGE: chrono::Duration = chrono::Duration::minutes(10);

    pub(crate) async fn recover_stale_pending_rows(&self) -> Result<u64, PersistError> {
        let cutoff = Utc::now() - Self::CRASH_RECOVERY_MIN_AGE;
        let result = sqlx::query!(
            "WITH upd AS (
                 UPDATE requests.requests r
                    SET status_id = error_s.id,
                        completed_at = NOW()
                   FROM requests.statuses error_s
                  WHERE error_s.code = 'error'
                    AND r.status_id = ANY (ARRAY[1, 2])
                    AND r.created_at <= $1
                  RETURNING r.id
             )
             INSERT INTO requests.errors (request_id, message)
             SELECT id, 'proxy_crashed_during_stream' FROM upd",
            cutoff,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query!(
            "UPDATE sessions.sessions s
             SET last_status = latest.status
             FROM (
                 SELECT DISTINCT ON (r.session_id)
                        r.session_id,
                        CASE st.code
                            WHEN 'pending'   THEN 'Pending'
                            WHEN 'streaming' THEN 'Streaming'
                            WHEN 'complete'  THEN 'Complete'
                            WHEN 'error'     THEN 'Error: ' || COALESCE(re.message, '')
                        END AS status
                 FROM requests.requests r
                 JOIN sessions.sessions sf ON sf.id = r.session_id
                     AND sf.last_status IN ('Pending', 'Streaming')
                 JOIN requests.statuses st ON st.id = r.status_id
                 LEFT JOIN requests.errors re ON re.request_id = r.id
                 ORDER BY r.session_id, r.created_at DESC, r.id DESC
             ) latest
             WHERE s.id = latest.session_id
               AND s.last_status IS DISTINCT FROM latest.status",
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub(crate) async fn mark_intercepted(
        &self,
        original_request_id: i64,
        correction_request_id: i64,
    ) {
        if crate::store::models::is_fallback(original_request_id as u64) {
            tracing::warn!(
                original_request_id,
                "mark_intercepted skipped: the original request was never persisted"
            );
            return;
        }
        let persisted_correction = persisted_request_id(correction_request_id);
        let result = sqlx::query!(
            "UPDATE requests.requests
             SET intercepted                    = TRUE,
                 correction_request_id          = $2
             WHERE id = $1",
            original_request_id,
            persisted_correction,
        )
        .execute(&self.pool)
        .await;
        report_mark_intercepted(result, original_request_id, correction_request_id);
    }
}

fn persisted_request_id(id: i64) -> Option<i64> {
    (!crate::store::models::is_fallback(id as u64)).then_some(id)
}

fn warn_mark_intercepted_no_row(original_request_id: i64, correction_request_id: i64) {
    tracing::warn!(
        original_request_id,
        correction_request_id,
        "mark_intercepted matched no request row"
    );
}

fn warn_mark_intercepted_failed(
    original_request_id: i64,
    correction_request_id: i64,
    error: &sqlx::Error,
) {
    tracing::warn!(
        original_request_id,
        correction_request_id,
        error = %error,
        "mark_intercepted DB update failed"
    );
}

fn report_mark_intercepted(
    result: Result<sqlx::postgres::PgQueryResult, sqlx::Error>,
    original_request_id: i64,
    correction_request_id: i64,
) {
    match result {
        Ok(done) if done.rows_affected() == 0 => {
            warn_mark_intercepted_no_row(original_request_id, correction_request_id);
        }
        Ok(_) => {}
        Err(e) => {
            warn_mark_intercepted_failed(original_request_id, correction_request_id, &e);
        }
    }
}
