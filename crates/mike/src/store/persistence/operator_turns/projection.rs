use sqlx::PgConnection;

use crate::store::persistence::turn_verdicts::TurnVerdictInsert;
use crate::store::persistence::{PersistError, PersistHandle};

pub const OUTCOME_PROJECTION_VERSION: i16 = 2;
pub const PROJECTION_BATCH: usize = 500;

pub const REACTIONS: [&str; 5] = [
    "corrected",
    "redirected",
    "accepted",
    "no_response",
    "unclear",
];
pub const DELIVERIES: [&str; 6] = [
    "shipped",
    "delivered",
    "partial",
    "none",
    "question",
    "not_applicable",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionMode {
    Apply,
    ResetWithoutVerdict,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectionParams {
    pub live_bundle_id: i64,
    pub confidence_threshold: f32,
    pub mode: ProjectionMode,
    pub session_judge_live_bundle_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromoteOutcome {
    Projected {
        turn_id: i64,
        outcome: String,
        changed: bool,
    },
    NotLive,
}

pub fn map_legacy_outcome(
    reaction: &str,
    delivery: &str,
    confidence: f32,
    threshold: f32,
    frustration_close: bool,
) -> &'static str {
    if frustration_close {
        return "abandoned";
    }
    let effective = if confidence < threshold {
        "unclear"
    } else {
        reaction
    };
    match (effective, delivery) {
        ("corrected", _) => "corrected",
        ("accepted" | "redirected", "shipped") => "shipped",
        ("accepted" | "redirected", _) => "accepted",
        (_, "shipped") => "shipped",
        _ => "unknown",
    }
}

pub(crate) async fn project_turn_outcomes_on(
    conn: &mut PgConnection,
    turn_ids: &[i64],
    params: &ProjectionParams,
) -> Result<Vec<(i64, String)>, PersistError> {
    let mut changed = Vec::new();
    for chunk in turn_ids.chunks(PROJECTION_BATCH) {
        let rows = sqlx::query_file!(
            "src/store/persistence/operator_turns/projection.sql",
            chunk,
            params.live_bundle_id,
            params.mode == ProjectionMode::ResetWithoutVerdict,
            OUTCOME_PROJECTION_VERSION,
            params.confidence_threshold,
            params.session_judge_live_bundle_id,
        )
        .fetch_all(&mut *conn)
        .await?;
        changed.extend(rows.into_iter().map(|row| (row.id, row.code)));
    }
    Ok(changed)
}

async fn current_outcome(conn: &mut PgConnection, turn_id: i64) -> Result<String, PersistError> {
    let code = sqlx::query_scalar!(
        "SELECT tot.code
           FROM memory.operator_turns ot
           JOIN memory.turn_outcomes tot ON tot.id = ot.outcome_id
          WHERE ot.id = $1",
        turn_id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    code.ok_or(PersistError::TurnNotFound(turn_id))
}

pub(crate) async fn promote_on(
    conn: &mut PgConnection,
    verdict_id: i64,
    params: &ProjectionParams,
) -> Result<PromoteOutcome, PersistError> {
    let row = sqlx::query!(
        "SELECT operator_turn_id, mode, bundle_id FROM memory.turn_verdicts WHERE id = $1",
        verdict_id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(verdict) = row else {
        return Err(PersistError::DataInvariant(format!(
            "turn verdict {verdict_id} does not exist"
        )));
    };
    let turn_id = verdict.operator_turn_id;
    if verdict.mode != "live" || verdict.bundle_id != Some(params.live_bundle_id) {
        return Ok(PromoteOutcome::NotLive);
    }
    let apply = ProjectionParams {
        mode: ProjectionMode::Apply,
        ..*params
    };
    let changed = project_turn_outcomes_on(conn, &[turn_id], &apply).await?;
    sqlx::query!(
        "UPDATE memory.ask_ledger_entries a
            SET kind = CASE ot.reaction
                           WHEN 'corrected' THEN 'correction'
                           WHEN 'redirected' THEN 'redirect'
                           ELSE 'clarification'
                       END,
                kind_source = CASE WHEN ot.outcome_source = 'operator' THEN 'operator' ELSE 'judge' END,
                kind_updated_at = now()
           FROM memory.operator_turns ot
           JOIN memory.turn_facts f ON f.operator_turn_id = ot.id
           JOIN memory.operator_turns nt ON nt.start_request_id = f.next_genuine_request_id
          WHERE ot.id = $1
            AND a.operator_turn_id = nt.id
            AND ot.reaction IN ('corrected', 'redirected', 'accepted', 'unclear')
            AND a.kind <> 'initial'
            AND a.kind_source <> 'operator'",
        turn_id,
    )
    .execute(&mut *conn)
    .await?;
    let outcome = match changed.first() {
        Some((_, code)) => code.clone(),
        None => current_outcome(conn, turn_id).await?,
    };
    Ok(PromoteOutcome::Projected {
        turn_id,
        outcome,
        changed: !changed.is_empty(),
    })
}

impl PersistHandle {
    pub async fn project_turn_outcomes(
        &self,
        turn_ids: &[i64],
        params: &ProjectionParams,
    ) -> Result<Vec<(i64, String)>, PersistError> {
        crate::metrics::time_db("project_turn_outcomes", async move {
            let mut conn = self.pool.acquire().await?;
            project_turn_outcomes_on(&mut conn, turn_ids, params).await
        })
        .await
    }

    pub async fn promote_turn_verdict(
        &self,
        verdict_id: i64,
        params: &ProjectionParams,
    ) -> Result<PromoteOutcome, PersistError> {
        crate::metrics::time_db("promote_turn_verdict", async move {
            let mut tx = self.pool.begin().await?;
            let outcome = promote_on(&mut tx, verdict_id, params).await?;
            tx.commit().await?;
            Ok(outcome)
        })
        .await
    }

    pub async fn insert_live_verdict_and_promote(
        &self,
        verdict: TurnVerdictInsert<'_>,
        params: &ProjectionParams,
    ) -> Result<(i64, PromoteOutcome), PersistError> {
        crate::metrics::time_db("insert_live_verdict_and_promote", async move {
            let projected =
                verdict.mode.as_str() == "live" && verdict.bundle_id == Some(params.live_bundle_id);
            let mut tx = self.pool.begin().await?;
            let verdict_id = crate::store::persistence::turn_verdicts::insert_turn_verdict_on(
                &mut tx, &verdict, projected,
            )
            .await?;
            let outcome = promote_on(&mut tx, verdict_id, params).await?;
            tx.commit().await?;
            Ok((verdict_id, outcome))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::{DELIVERIES, REACTIONS, map_legacy_outcome};

    #[test]
    fn map_legacy_outcome_corrected_reaction_is_corrected_for_every_delivery() {
        for delivery in DELIVERIES {
            assert_eq!(
                map_legacy_outcome("corrected", delivery, 0.9, 0.5, false),
                "corrected"
            );
        }
    }

    #[test]
    fn map_legacy_outcome_accepted_shipped_is_shipped() {
        assert_eq!(
            map_legacy_outcome("accepted", "shipped", 0.9, 0.5, false),
            "shipped"
        );
        assert_eq!(
            map_legacy_outcome("redirected", "shipped", 0.9, 0.5, false),
            "shipped"
        );
    }

    #[test]
    fn map_legacy_outcome_redirected_partial_is_accepted() {
        assert_eq!(
            map_legacy_outcome("redirected", "partial", 0.9, 0.5, false),
            "accepted"
        );
        for delivery in ["delivered", "partial", "question", "not_applicable", "none"] {
            assert_eq!(
                map_legacy_outcome("accepted", delivery, 0.9, 0.5, false),
                "accepted"
            );
        }
    }

    #[test]
    fn map_legacy_outcome_no_response_delivered_is_unknown() {
        assert_eq!(
            map_legacy_outcome("no_response", "delivered", 0.9, 0.5, false),
            "unknown"
        );
        assert_eq!(
            map_legacy_outcome("unclear", "shipped", 0.9, 0.5, false),
            "shipped"
        );
        assert_eq!(
            map_legacy_outcome("no_response", "shipped", 0.9, 0.5, false),
            "shipped"
        );
    }

    #[test]
    fn map_legacy_outcome_low_confidence_corrected_is_unknown() {
        assert_eq!(
            map_legacy_outcome("corrected", "partial", 0.2, 0.5, false),
            "unknown"
        );
        assert_eq!(
            map_legacy_outcome("corrected", "shipped", 0.2, 0.5, false),
            "shipped"
        );
    }

    #[test]
    fn map_legacy_outcome_writes_abandoned_only_for_a_frustration_close() {
        for reaction in REACTIONS {
            for delivery in DELIVERIES {
                assert_eq!(
                    map_legacy_outcome(reaction, delivery, 0.9, 0.5, true),
                    "abandoned"
                );
            }
        }
    }

    #[test]
    fn map_legacy_outcome_never_returns_abandoned() {
        for reaction in REACTIONS {
            for delivery in DELIVERIES {
                for confidence in [0.9, 0.1] {
                    let code = map_legacy_outcome(reaction, delivery, confidence, 0.5, false);
                    assert_ne!(code, "abandoned", "{reaction}/{delivery}/{confidence}");
                    assert!(
                        ["corrected", "shipped", "accepted", "unknown"].contains(&code),
                        "{code}"
                    );
                }
            }
        }
    }
}
