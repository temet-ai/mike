use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactionSource {
    V2,
    LegacyR3,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveReaction {
    pub turn_id: i64,
    pub verdict_id: i64,
    pub bundle_id: Option<i64>,
    pub source: ReactionSource,
    pub reaction: Option<String>,
    pub delivery: Option<String>,
    pub legacy_label: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct LiveReactionRow {
    turn_id: i64,
    verdict_id: i64,
    bundle_id: Option<i64>,
    is_v2: bool,
    reaction: Option<String>,
    delivery: Option<String>,
    legacy_label: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct LegacyLabelRow {
    pub turn_id: i64,
    pub outcome: Option<String>,
    pub classified_at: Option<DateTime<Utc>>,
    pub judge_model: Option<String>,
    pub judge_rubric_version: Option<String>,
    pub rationale: Option<String>,
    pub unmet_ask: Option<String>,
    pub evidence_quality: Option<String>,
    pub judge_failure_reason: Option<String>,
    pub snapshot_at: DateTime<Utc>,
}

impl From<LiveReactionRow> for LiveReaction {
    fn from(r: LiveReactionRow) -> Self {
        Self {
            turn_id: r.turn_id,
            verdict_id: r.verdict_id,
            bundle_id: r.bundle_id,
            source: if r.is_v2 {
                ReactionSource::V2
            } else {
                ReactionSource::LegacyR3
            },
            reaction: r.reaction,
            delivery: r.delivery,
            legacy_label: r.legacy_label,
            created_at: r.created_at,
        }
    }
}

impl PersistHandle {
    pub async fn turn_live_verdict_id(&self, turn_id: i64) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("turn_live_verdict_id", async move {
            let id: Option<Option<i64>> = sqlx::query_scalar!(
                "SELECT live_verdict_id FROM memory.operator_turns WHERE id = $1",
                turn_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(id.flatten())
        })
        .await
    }

    pub async fn live_reactions_as_of(
        &self,
        turn_ids: &[i64],
        as_of: DateTime<Utc>,
    ) -> Result<Vec<LiveReaction>, PersistError> {
        crate::metrics::time_db("live_reactions_as_of", async move {
            let rows = sqlx::query_file_as!(
                LiveReactionRow,
                "src/store/persistence/operator_turns/live_reactions.sql",
                turn_ids,
                as_of,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(LiveReaction::from).collect())
        })
        .await
    }

    pub async fn legacy_label_for_turn(
        &self,
        turn_id: i64,
    ) -> Result<Option<LegacyLabelRow>, PersistError> {
        crate::metrics::time_db("legacy_label_for_turn", async move {
            Ok(sqlx::query_as!(
                LegacyLabelRow,
                "SELECT l.turn_id, tot.code AS outcome, l.classified_at, l.judge_model,
                        l.judge_rubric_version, l.rationale, l.unmet_ask, l.evidence_quality,
                        l.judge_failure_reason, l.snapshot_at
                   FROM memory.operator_turn_legacy_labels l
                   LEFT JOIN memory.turn_outcomes tot ON tot.id = l.outcome_id
                  WHERE l.turn_id = $1",
                turn_id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }
}
