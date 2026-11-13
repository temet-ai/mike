use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::PersistError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProposerParams<'a> {
    pub window_days: i32,
    pub min_sessions: i32,
    pub min_confirmed: i32,
    pub rollup_id: i64,
    pub version: &'a str,
    pub reject_cooldown_days: i32,
    pub max_new_per_run: i64,
    pub max_open: i64,
    pub operator_min_scopes: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct ProposalRow {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub source: String,
    pub scope: String,
    pub project_id: Option<i64>,
    pub class: String,
    pub polarity: String,
    pub status: String,
    pub proposed_kind: String,
    pub proposed_text: String,
    pub evidence_count: i32,
    pub session_count: i32,
    pub first_evidence_at: DateTime<Utc>,
    pub last_evidence_at: DateTime<Utc>,
    pub window_days: i32,
    pub rollup_id: i64,
    pub proposer_version: String,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decision_note: Option<String>,
    pub approved_text: Option<String>,
    pub superseded_by: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct ProposalEvidenceRow {
    pub proposal_id: i64,
    pub kind: String,
    pub ref_id: i64,
    pub operator_turn_id: Option<i64>,
    pub session_id: Option<i64>,
    pub observed_at: DateTime<Utc>,
    pub detail: serde_json::Value,
    pub message_text: Option<String>,
    pub gone: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProposalDetail {
    pub proposal: ProposalRow,
    pub evidence: Vec<ProposalEvidenceRow>,
}

fn distinct_count(mut ids: Vec<i64>) -> u64 {
    ids.sort_unstable();
    ids.dedup();
    u64::try_from(ids.len()).unwrap_or(0)
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn propose_repeated_directives(
        self,
        p: &ProposerParams<'_>,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("propose_repeated_directives", async move {
            let ids = sqlx::query_file_scalar!(
                "src/store/persistence/cross_session/repeated_directive.sql",
                p.window_days,
                i64::from(p.min_sessions),
                p.rollup_id,
                p.version,
                p.reject_cooldown_days,
                p.max_new_per_run,
                p.max_open,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(distinct_count(ids))
        })
        .await
    }

    pub async fn propose_confirmed_handbacks(
        self,
        p: &ProposerParams<'_>,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("propose_confirmed_handbacks", async move {
            let ids = sqlx::query_file_scalar!(
                "src/store/persistence/cross_session/confirmed_handback.sql",
                p.window_days,
                i64::from(p.min_confirmed),
                p.rollup_id,
                p.version,
                p.reject_cooldown_days,
                p.max_new_per_run,
                p.max_open,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(distinct_count(ids))
        })
        .await
    }

    pub async fn roll_up_operator_scope(self, p: &ProposerParams<'_>) -> Result<u64, PersistError> {
        crate::metrics::time_db("roll_up_operator_scope", async move {
            let ids = sqlx::query_file_scalar!(
                "src/store/persistence/cross_session/operator_rollup.sql",
                p.operator_min_scopes,
                p.window_days,
                p.rollup_id,
                p.version,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(u64::try_from(ids.len()).unwrap_or(0))
        })
        .await
    }

    pub async fn list_directive_proposals(
        self,
        status: Option<&str>,
        before_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<ProposalRow>, PersistError> {
        crate::metrics::time_db("list_directive_proposals", async move {
            let before_id = before_id.unwrap_or(i64::MAX);
            Ok(match status {
                Some(status) => {
                    sqlx::query_file_as!(
                        ProposalRow,
                        "src/store/persistence/cross_session/proposals_by_status.sql",
                        status,
                        before_id,
                        limit,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_file_as!(
                        ProposalRow,
                        "src/store/persistence/cross_session/proposals_all.sql",
                        before_id,
                        limit,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            })
        })
        .await
    }

    pub async fn fetch_proposal_detail(
        self,
        id: i64,
    ) -> Result<Option<ProposalDetail>, PersistError> {
        crate::metrics::time_db("fetch_proposal_detail", async move {
            let proposal = sqlx::query_file_as!(
                ProposalRow,
                "src/store/persistence/cross_session/proposal_by_id.sql",
                id,
            )
            .fetch_optional(&self.pool)
            .await?;
            let Some(proposal) = proposal else {
                return Ok(None);
            };
            let evidence = sqlx::query_as!(
                ProposalEvidenceRow,
                r#"SELECT ev.proposal_id, ev.kind, ev.ref_id, ev.operator_turn_id, ev.session_id,
                          ev.observed_at, ev.detail, t.user_prompt AS "message_text?",
                          (t.id IS NULL) AS "gone!"
                     FROM memory.directive_proposal_evidence ev
                     LEFT JOIN memory.operator_turns t ON t.id = ev.operator_turn_id
                    WHERE ev.proposal_id = $1
                    ORDER BY ev.observed_at, ev.ref_id"#,
                id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(Some(ProposalDetail { proposal, evidence }))
        })
        .await
    }

    pub async fn reject_directive_proposal(
        self,
        id: i64,
        note: Option<&str>,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("reject_directive_proposal", async move {
            let rejected = sqlx::query!(
                "UPDATE memory.directive_proposals
                    SET status = 'rejected', decided_at = now(), decision_note = $2
                  WHERE id = $1 AND status = 'open'",
                id,
                note,
            )
            .execute(&self.pool)
            .await?;
            Ok(rejected.rows_affected() > 0)
        })
        .await
    }
}
