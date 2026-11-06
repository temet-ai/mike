use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Serialize)]
pub struct ProposalProjectRow {
    pub project_uuid: Uuid,
    pub project_path: String,
    pub display_name: String,
    pub proposals: i64,
    pub undecided: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlacementProposalRow {
    pub lesson_id: i64,
    pub placement: String,
    pub kind: String,
    pub stage: String,
    pub text: String,
    pub topic: Option<String>,
    pub covered_by: Option<String>,
    pub reason: Option<String>,
    pub agent_name: Option<String>,
    pub user_prompt: Option<String>,
    pub created_at: DateTime<Utc>,
    pub placed_at: Option<DateTime<Utc>>,
    pub decision: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementDecision {
    Accepted,
    Declined,
    Cleared,
}

impl PlacementDecision {
    fn code(self) -> Option<&'static str> {
        match self {
            Self::Accepted => Some("accepted"),
            Self::Declined => Some("declined"),
            Self::Cleared => None,
        }
    }
}

impl PersistHandle {
    pub async fn list_proposal_projects(&self) -> Result<Vec<ProposalProjectRow>, PersistError> {
        crate::metrics::time_db("list_proposal_projects", async move {
            Ok(sqlx::query_as!(
                ProposalProjectRow,
                r#"SELECT p.project_uuid AS "project_uuid!", p.project_path AS "project_path!",
                          coalesce(p.custom_name, p.display_name) AS "display_name!",
                          count(*) AS "proposals!",
                          count(*) FILTER (WHERE r.lesson_id IS NULL) AS "undecided!"
                     FROM memory.lessons l
                     JOIN memory.lesson_placements pl ON pl.id = l.placement_id
                     JOIN projects.projects p ON p.id = l.project_id
                     LEFT JOIN memory.lesson_placement_reviews r ON r.lesson_id = l.id
                    WHERE pl.code IN ('rule', 'agent') AND l.archived_at IS NULL
                    GROUP BY p.id
                    ORDER BY p.project_path, p.id"#
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn list_placement_proposals(
        &self,
        project_uuid: Uuid,
    ) -> Result<Vec<PlacementProposalRow>, PersistError> {
        crate::metrics::time_db("list_placement_proposals", async move {
            Ok(sqlx::query_as!(
                PlacementProposalRow,
                r#"SELECT l.id AS lesson_id, pl.code AS placement, k.code AS kind, st.code AS stage,
                          l.text, l.topic, l.placement_covered_by AS covered_by,
                          l.placement_reason AS reason, a.name AS "agent_name?",
                          t.user_prompt AS "user_prompt?", l.created_at, l.placed_at,
                          r.decision AS "decision?", r.decided_at AS "decided_at?"
                     FROM memory.lessons l
                     JOIN projects.projects p ON p.id = l.project_id
                     JOIN memory.lesson_placements pl ON pl.id = l.placement_id
                     JOIN memory.lesson_kinds k ON k.id = l.kind_id
                     JOIN memory.lesson_stages st ON st.id = l.stage_id
                     LEFT JOIN agents.agents a ON a.id = l.agent_id
                     LEFT JOIN memory.operator_turns t ON t.id = l.source_turn_id
                     LEFT JOIN memory.lesson_placement_reviews r ON r.lesson_id = l.id
                    WHERE p.project_uuid = $1 AND pl.code IN ('rule', 'agent')
                      AND l.archived_at IS NULL
                    ORDER BY l.id"#,
                project_uuid
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn set_placement_decision(
        &self,
        lesson_id: i64,
        decision: PlacementDecision,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("set_placement_decision", async move {
            let proposal = sqlx::query_scalar!(
                r#"SELECT EXISTS (
                       SELECT 1 FROM memory.lessons l
                         JOIN memory.lesson_placements pl ON pl.id = l.placement_id
                        WHERE l.id = $1 AND pl.code IN ('rule', 'agent')) AS "found!""#,
                lesson_id
            )
            .fetch_one(&self.pool)
            .await?;
            if !proposal {
                return Ok(false);
            }
            match decision.code() {
                Some(code) => {
                    sqlx::query!(
                        "INSERT INTO memory.lesson_placement_reviews (lesson_id, decision)
                         VALUES ($1, $2)
                         ON CONFLICT (lesson_id) DO UPDATE
                            SET decision = EXCLUDED.decision, decided_at = now()",
                        lesson_id,
                        code
                    )
                    .execute(&self.pool)
                    .await?;
                }
                None => {
                    sqlx::query!(
                        "DELETE FROM memory.lesson_placement_reviews WHERE lesson_id = $1",
                        lesson_id
                    )
                    .execute(&self.pool)
                    .await?;
                }
            }
            Ok(true)
        })
        .await
    }
}
