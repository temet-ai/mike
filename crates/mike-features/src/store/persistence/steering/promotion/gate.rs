use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::super::PersistError;

#[derive(Debug, Clone, PartialEq)]
pub struct GateWindow<'a> {
    pub steerer_id: i64,
    pub question: &'a str,
    pub bundle_id: Option<i64>,
    pub since: DateTime<Utc>,
    pub grace_secs: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, sqlx::FromRow)]
pub struct GateCounts {
    pub would_fire: i64,
    pub labelled_right: i64,
    pub labelled_wrong: i64,
    pub labelled_indeterminate: i64,
    pub source_next_message: i64,
    pub source_session_end: i64,
    pub source_operator: i64,
    pub source_replay: i64,
    pub with_quorum: i64,
    pub operator_fires: i64,
    pub template_fires: i64,
    pub other_fires: i64,
    pub non_actor_fires: i64,
    pub timing_n: i64,
    pub within_grace: i64,
    pub within_grace_60: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, sqlx::FromRow)]
pub struct VerifierPairs {
    pub turns: i64,
    pub both_fired: i64,
    pub verifier_only: i64,
    pub guard_only: i64,
    pub neither: i64,
    pub verifier_counted: i64,
    pub verifier_corrected: i64,
    pub guard_corrected: i64,
    pub relevant: i64,
    pub verifier_hits: i64,
    pub guard_hits: i64,
    pub verifier_calls: i64,
    pub verifier_usd: f64,
    pub guard_calls: i64,
    pub guard_usd: f64,
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn fetch_gate_counts(self, w: &GateWindow<'_>) -> Result<GateCounts, PersistError> {
        crate::metrics::time_db("fetch_gate_counts", async move {
            Ok(match w.bundle_id {
                Some(bundle_id) => {
                    sqlx::query_file_as!(
                        GateCounts,
                        "src/store/persistence/steering/promotion/gate_counts.sql",
                        w.steerer_id,
                        w.question,
                        bundle_id,
                        w.since,
                        w.grace_secs,
                    )
                    .fetch_one(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_file_as!(
                        GateCounts,
                        "src/store/persistence/steering/promotion/gate_counts_all_bundles.sql",
                        w.steerer_id,
                        w.question,
                        w.since,
                        w.grace_secs,
                    )
                    .fetch_one(&self.pool)
                    .await?
                }
            })
        })
        .await
    }

    pub async fn fetch_live_precision_window(
        self,
        steerer_id: i64,
        question: &str,
        n: i64,
    ) -> Result<(i64, i64), PersistError> {
        crate::metrics::time_db("fetch_live_precision_window", async move {
            let row = sqlx::query!(
                r#"WITH labelled AS (
                       SELECT e.outcome_label
                         FROM steering.evaluations e
                        WHERE e.steerer_id = $1 AND e.question = $2 AND e.mode = 'live'
                          AND e.status = 'resolved_correction'
                          AND e.outcome_label IN ('right', 'wrong')
                          AND (e.arm = 'holdback'
                               OR (e.arm = 'treated'
                                   AND e.label_source IN ('operator', 'session_reply')))
                        ORDER BY e.labelled_at DESC, e.id DESC
                        LIMIT $3
                   )
                   SELECT count(*) FILTER (WHERE outcome_label = 'right')::bigint AS "right!",
                          count(*) FILTER (WHERE outcome_label = 'wrong')::bigint AS "wrong!"
                     FROM labelled"#,
                steerer_id,
                question,
                n,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok((row.right, row.wrong))
        })
        .await
    }

    pub async fn fetch_verifier_pairs(
        self,
        verifier_id: i64,
        guard_id: i64,
        since: DateTime<Utc>,
    ) -> Result<VerifierPairs, PersistError> {
        crate::metrics::time_db("fetch_verifier_pairs", async move {
            Ok(sqlx::query_file_as!(
                VerifierPairs,
                "src/store/persistence/steering/promotion/verifier_pairs.sql",
                verifier_id,
                guard_id,
                since,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn fetch_wake_counts(
        self,
        steerer_id: i64,
        question: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<(i64, i64), PersistError> {
        crate::metrics::time_db("fetch_wake_counts", async move {
            let row = sqlx::query!(
                r#"SELECT count(*) FILTER (WHERE i.wake_state = 'pushed')::bigint AS "pushed!",
                          count(*) FILTER (WHERE i.wake_state = 'pushed'
                                             AND e.outcome_label = 'wrong'
                                             AND e.label_source IN ('operator', 'session_reply'))::bigint
                              AS "wrong!"
                     FROM steering.evaluations e
                     JOIN steering.interventions i ON i.uuid = e.intervention_uuid
                    WHERE e.steerer_id = $1 AND e.question = $2 AND e.arm = 'treated'
                      AND e.dispatched_at >= $3 AND e.dispatched_at < $4"#,
                steerer_id,
                question,
                from,
                to,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok((row.pushed, row.wrong))
        })
        .await
    }
}
