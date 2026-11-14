use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::PersistError;

#[derive(Debug, Clone, PartialEq)]
pub struct TurnJudgeItem {
    pub turn_id: i64,
    pub session_id: i64,
    pub closed_at: DateTime<Utc>,
    pub outcome: String,
    pub gold_reaction: Option<String>,
    pub gold_delivery: Option<String>,
    pub live_reaction: Option<String>,
    pub live_delivery: Option<String>,
    pub live_legacy_label: Option<String>,
    pub live_confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorrectionItem {
    pub entry_id: i64,
    pub session_id: i64,
    pub request_id: i64,
    pub kind: String,
    pub keyword_kind: Option<String>,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifierItem {
    pub evaluation_uuid: Uuid,
    pub intervention_uuid: Uuid,
    pub session_id: i64,
    pub operator_turn_id: i64,
    pub trigger_request_id: Option<i64>,
    pub label: String,
    pub label_source: String,
    pub nudge: String,
    pub verdict_json: Option<serde_json::Value>,
    pub operator_ask: String,
    pub turn_end: String,
    pub dispatched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WakeItem {
    pub evaluation_uuid: Uuid,
    pub intervention_uuid: Uuid,
    pub session_id: i64,
    pub operator_turn_id: i64,
    pub question: Option<String>,
    pub label: String,
    pub label_source: String,
    pub wake_state: Option<String>,
    pub nudge: String,
    pub decided_at: DateTime<Utc>,
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub async fn replay_turn_judge_items(
        self,
        since: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<TurnJudgeItem>, PersistError> {
        crate::metrics::time_db("replay_turn_judge_items", async move {
            Ok(sqlx::query_as!(
                TurnJudgeItem,
                r#"WITH gold AS (
                       SELECT DISTINCT ON (l.subject_id, l.axis)
                              l.subject_id AS turn_id, l.axis, l.label
                         FROM eval.labels_as_of('infinity'::timestamptz) l
                        WHERE l.subject_kind = 'turn' AND l.tier = 'gold'
                          AND l.axis IN ('reaction', 'delivery')
                        ORDER BY l.subject_id, l.axis, l.created_at DESC, l.id DESC
                   ),
                   axes AS (
                       SELECT g.turn_id,
                              max(g.label) FILTER (WHERE g.axis = 'reaction') AS reaction,
                              max(g.label) FILTER (WHERE g.axis = 'delivery') AS delivery
                         FROM gold g
                        GROUP BY g.turn_id
                   )
                   SELECT t.id AS "turn_id!", t.session_id AS "session_id!",
                          t.closed_at AS "closed_at!", o.code AS "outcome!",
                          a.reaction AS gold_reaction, a.delivery AS gold_delivery,
                          v.reaction AS live_reaction, v.delivery AS live_delivery,
                          v.legacy_label AS live_legacy_label, v.confidence AS "live_confidence?"
                     FROM memory.operator_turns t
                     JOIN memory.turn_outcomes o ON o.id = t.outcome_id
                     JOIN axes a ON a.turn_id = t.id
                     LEFT JOIN LATERAL (
                         SELECT x.reaction, x.delivery, x.legacy_label, x.confidence
                           FROM memory.turn_verdicts x
                          WHERE x.operator_turn_id = t.id AND x.mode = 'live'
                          ORDER BY x.created_at DESC, x.id DESC
                          LIMIT 1
                     ) v ON true
                    WHERE t.outcome_source = 'operator'
                      AND t.closed_at IS NOT NULL
                      AND t.closed_at >= $1
                    ORDER BY t.id DESC
                    LIMIT $2"#,
                since,
                limit,
            )
            .fetch_all(self.pool())
            .await?)
        })
        .await
    }

    pub async fn replay_correction_items(
        self,
        since: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<CorrectionItem>, PersistError> {
        crate::metrics::time_db("replay_correction_items", async move {
            Ok(sqlx::query_as!(
                CorrectionItem,
                r#"SELECT e.id AS "entry_id!", e.session_id AS "session_id!",
                          e.request_id AS "request_id!", e.kind AS "kind!", e.keyword_kind,
                          e.content AS "content!", e.created_at AS "created_at!"
                     FROM memory.ask_ledger_entries e
                    WHERE e.kind_source = 'operator'
                      AND e.kind IN ('correction', 'clarification')
                      AND e.request_id IS NOT NULL
                      AND e.created_at >= $1
                    ORDER BY e.id DESC
                    LIMIT $2"#,
                since,
                limit,
            )
            .fetch_all(self.pool())
            .await?)
        })
        .await
    }

    pub async fn replay_verifier_items(
        self,
        since: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<VerifierItem>, PersistError> {
        crate::metrics::time_db("replay_verifier_items", async move {
            Ok(sqlx::query_as!(
                VerifierItem,
                r#"SELECT e.uuid AS "evaluation_uuid!", i.uuid AS "intervention_uuid!",
                          e.session_id AS "session_id!", e.operator_turn_id AS "operator_turn_id!",
                          i.trigger_request_id, e.outcome_label AS "label!",
                          e.label_source AS "label_source!", i.payload_text AS "nudge!",
                          i.verdict_json,
                          COALESCE(ot.user_prompt, '') AS "operator_ask!",
                          COALESCE(ot.final_assistant_text, '') AS "turn_end!",
                          e.dispatched_at AS "dispatched_at!"
                     FROM steering.interventions i
                     JOIN steering.steerers st ON st.id = i.steerer_id AND st.kind = 'verifier'
                     JOIN steering.evaluations e ON e.intervention_uuid = i.uuid
                     JOIN memory.operator_turns ot ON ot.id = e.operator_turn_id
                    WHERE e.status = 'resolved_correction'
                      AND e.outcome_label IN ('right', 'wrong')
                      AND i.payload_text IS NOT NULL
                      AND e.dispatched_at >= $1
                    ORDER BY e.id DESC
                    LIMIT $2"#,
                since,
                limit,
            )
            .fetch_all(self.pool())
            .await?)
        })
        .await
    }

    pub async fn replay_wake_items(
        self,
        since: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<WakeItem>, PersistError> {
        crate::metrics::time_db("replay_wake_items", async move {
            Ok(sqlx::query_as!(
                WakeItem,
                r#"SELECT e.uuid AS "evaluation_uuid!", i.uuid AS "intervention_uuid!",
                          e.session_id AS "session_id!", e.operator_turn_id AS "operator_turn_id!",
                          e.question, e.outcome_label AS "label!",
                          e.label_source AS "label_source!", i.wake_state,
                          i.payload_text AS "nudge!",
                          COALESCE(i.wake_resolved_at, i.deliver_after, i.created_at)
                              AS "decided_at!"
                     FROM steering.evaluations e
                     JOIN steering.steerers st ON st.id = e.steerer_id AND st.kind = 'turn_guard'
                     JOIN steering.interventions i ON i.uuid = e.intervention_uuid
                    WHERE e.status = 'resolved_correction'
                      AND i.delivery_channel = 'wake'
                      AND e.outcome_label IN ('right', 'wrong')
                      AND i.payload_text IS NOT NULL
                      AND e.dispatched_at >= $1
                    ORDER BY e.id DESC
                    LIMIT $2"#,
                since,
                limit,
            )
            .fetch_all(self.pool())
            .await?)
        })
        .await
    }
}
