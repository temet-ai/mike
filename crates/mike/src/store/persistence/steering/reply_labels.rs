use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReplyLabelCandidate {
    pub uuid: uuid::Uuid,
    pub steerer_id: i64,
    pub steerer_name: String,
    pub steerer_rule: Option<String>,
    pub session_id: i64,
    pub operator_turn_id: i64,
    pub question: Option<String>,
    pub payload_text: Option<String>,
    pub trigger_request_id: Option<i64>,
    pub delivered_request_id: Option<i64>,
    pub reply_turn_id: Option<i64>,
    pub reverted: bool,
    pub revert_reason: Option<String>,
}

impl PersistHandle {
    pub async fn reply_label_candidates(
        &self,
        limit: i64,
        since: DateTime<Utc>,
        regrade: RegradeOlderVersions<'_>,
    ) -> Result<Vec<ReplyLabelCandidate>, PersistError> {
        crate::metrics::time_db("reply_label_candidates", async move {
            let rows = sqlx::query_as!(
                ReplyLabelCandidate,
                r#"SELECT e.uuid, e.steerer_id, s.name AS steerer_name, s.prompt AS steerer_rule, e.session_id,
                          e.operator_turn_id, e.question, i.payload_text, i.trigger_request_id,
                          i.delivered_request_id, r.operator_turn_id AS reply_turn_id,
                          (i.reverted_at IS NOT NULL) AS "reverted!",
                          i.revert_reason
                     FROM steering.evaluations e
                     JOIN steering.interventions i ON i.uuid = e.intervention_uuid
                     JOIN steering.verdicts v ON v.id = i.verdict_id AND v.code = 'fired'
                     JOIN steering.steerers s ON s.id = e.steerer_id
                     LEFT JOIN requests.requests r ON r.id = i.delivered_request_id
                     LEFT JOIN memory.operator_turns rt ON rt.id = r.operator_turn_id
                    WHERE e.intervention_uuid IS NOT NULL
                      AND (e.outcome_label IS NULL
                           OR (e.label_source = 'session_reply'
                               AND e.label_evidence->>'labeller_version' IS DISTINCT FROM $3
                               AND e.dispatched_at >= $4))
                      AND e.status <> 'dispatched'
                      AND e.dispatched_at >= $2
                      AND ((i.delivered_request_id IS NOT NULL AND r.completed_at IS NOT NULL
                            AND (rt.closed_at IS NOT NULL
                                 OR r.completed_at < now() - make_interval(secs => $5)))
                           OR (i.delivered_request_id IS NULL
                               AND (i.reverted_at IS NOT NULL OR i.expires_at < now())))
                    ORDER BY e.dispatched_at, e.id
                    LIMIT $1"#,
                limit,
                since,
                regrade.current_version,
                regrade.since,
                regrade.reply_settles_after_secs,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RegradeOlderVersions<'a> {
    pub current_version: &'a str,
    pub since: DateTime<Utc>,
    pub reply_settles_after_secs: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct SteererLabelWindow {
    pub(crate) steerer_id: i64,
    pub(crate) name: String,
    pub(crate) window_start: DateTime<Utc>,
    pub(crate) right: i64,
    pub(crate) wrong: i64,
}

impl PersistHandle {
    pub(crate) async fn steerer_label_windows(
        &self,
        floor: DateTime<Utc>,
    ) -> Result<Vec<SteererLabelWindow>, PersistError> {
        crate::metrics::time_db("steerer_label_windows", async move {
            let rows = sqlx::query_as!(
                SteererLabelWindow,
                r#"SELECT s.id AS steerer_id, s.name,
                          GREATEST(s.updated_at, $1) AS "window_start!",
                          count(e.id) FILTER (WHERE e.outcome_label = 'right') AS "right!",
                          count(e.id) FILTER (WHERE e.outcome_label = 'wrong') AS "wrong!"
                     FROM steering.steerers s
                     LEFT JOIN steering.evaluations e
                            ON e.steerer_id = s.id
                           AND e.intervention_uuid IS NOT NULL
                           AND e.label_source IN ('session_reply', 'operator')
                           AND e.dispatched_at >= GREATEST(s.updated_at, $1)
                    WHERE s.enabled AND s.deleted_at IS NULL AND s.kind <> 'turn_guard'
                    GROUP BY s.id, s.name, s.updated_at
                    ORDER BY s.id"#,
                floor,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
