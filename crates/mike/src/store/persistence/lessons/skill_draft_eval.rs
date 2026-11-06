use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::skill_drafts::DraftLesson;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq)]
pub struct EvalDraft {
    pub id: i64,
    pub project_id: i64,
    pub folder: String,
    pub description: String,
    pub skill_md: String,
    pub reference_files: Value,
    pub trigger_queries: Value,
    pub lesson_ids: Vec<i64>,
    pub agent_id: Option<i64>,
    pub eval_progress: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentScope {
    pub name: String,
    pub description: Option<String>,
    pub definition_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShippedPatch<'a> {
    pub description: &'a str,
    pub skill_md: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiblingSkill {
    pub folder: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacedDraft {
    pub folder: String,
    pub description: String,
    pub skill_md: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvalFailure {
    pub attempts: i32,
    pub rejected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListingWindow {
    pub candidates: i64,
    pub sessions: i64,
    pub min_bytes: i32,
    pub max_bytes: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingBody {
    pub request_id: i64,
    pub body: Vec<u8>,
}

impl PersistHandle {
    pub async fn claim_skill_draft_for_eval(&self) -> Result<Option<EvalDraft>, PersistError> {
        crate::metrics::time_db("claim_skill_draft_for_eval", async move {
            let row = sqlx::query!(
                r#"UPDATE memory.skill_drafts d
                      SET status = 'evaluating'
                    WHERE d.id = (
                          SELECT c.id
                            FROM memory.skill_drafts c
                            LEFT JOIN memory.skill_draft_eval_failures f ON f.draft_id = c.id
                           WHERE c.status IN ('draft', 'evaluating')
                             AND (f.draft_id IS NULL
                                  OR f.last_failed_at <= now() - interval '1 hour' * (2 ^ f.attempts))
                           ORDER BY (c.status = 'evaluating') DESC, c.created_at, c.id
                           LIMIT 1
                             FOR UPDATE OF c SKIP LOCKED)
                RETURNING d.id, d.project_id, d.folder, d.description, d.skill_md,
                          d.reference_files, d.trigger_queries,
                          d.lesson_ids, d.agent_id, d.eval_progress"#
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| EvalDraft {
                id: r.id,
                project_id: r.project_id,
                folder: r.folder,
                description: r.description,
                skill_md: r.skill_md,
                reference_files: r.reference_files,
                trigger_queries: r.trigger_queries,
                lesson_ids: r.lesson_ids,
                agent_id: r.agent_id,
                eval_progress: r.eval_progress,
            }))
        })
        .await
    }

    pub async fn skill_draft_agent_scope(
        &self,
        agent_id: i64,
    ) -> Result<Option<AgentScope>, PersistError> {
        crate::metrics::time_db("skill_draft_agent_scope", async move {
            let row = sqlx::query!(
                "SELECT a.name, a.description, a.definition_path
                   FROM agents.agents a
                  WHERE a.id = $1",
                agent_id
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| AgentScope {
                name: r.name,
                description: r.description,
                definition_path: r.definition_path,
            }))
        })
        .await
    }

    pub async fn skill_draft_lessons_by_id(
        &self,
        ids: &[i64],
    ) -> Result<Vec<DraftLesson>, PersistError> {
        crate::metrics::time_db("skill_draft_lessons_by_id", async move {
            let rows = sqlx::query!(
                r#"SELECT l.id, k.code AS "kind!", l.text, t.user_prompt
                     FROM memory.lessons l
                     JOIN memory.lesson_kinds k ON k.id = l.kind_id
                     LEFT JOIN memory.operator_turns t ON t.id = l.source_turn_id
                    WHERE l.id = ANY($1)
                    ORDER BY l.id"#,
                ids
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| DraftLesson {
                    id: r.id,
                    kind: r.kind,
                    text: r.text,
                    user_prompt: r.user_prompt.filter(|p| !p.trim().is_empty()),
                })
                .collect())
        })
        .await
    }

    pub async fn skill_draft_siblings(
        &self,
        project_id: i64,
        folder: &str,
        limit: i64,
    ) -> Result<Vec<SiblingSkill>, PersistError> {
        crate::metrics::time_db("skill_draft_siblings", async move {
            let rows = sqlx::query!(
                r#"SELECT s.folder AS "folder!", s.description AS "description!"
                     FROM (SELECT DISTINCT ON (d.folder)
                                  d.folder, d.description, d.status, d.created_at
                             FROM memory.skill_drafts d
                            WHERE d.project_id = $1
                              AND d.folder <> $2
                              AND d.status <> 'rejected'
                            ORDER BY d.folder, d.lesson_watermark DESC) s
                    ORDER BY (s.status = 'accepted') DESC, s.created_at DESC, s.folder
                    LIMIT $3"#,
                project_id,
                folder,
                limit
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| SiblingSkill {
                    folder: r.folder,
                    description: r.description,
                })
                .collect())
        })
        .await
    }

    pub async fn skill_draft_replaced(
        &self,
        project_id: i64,
        folders: &[String],
        draft_id: i64,
    ) -> Result<Option<ReplacedDraft>, PersistError> {
        crate::metrics::time_db("skill_draft_replaced", async move {
            let row = sqlx::query!(
                r#"SELECT d.folder, d.description, d.skill_md
                     FROM memory.skill_drafts d
                    WHERE d.project_id = $1
                      AND d.folder = ANY($2)
                      AND d.id < $3
                      AND ((d.status = 'accepted' AND d.review IS NULL) OR d.review = 'approved')
                    ORDER BY d.lesson_watermark DESC, d.revision DESC
                    LIMIT 1"#,
                project_id,
                folders,
                draft_id
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| ReplacedDraft {
                folder: r.folder,
                description: r.description,
                skill_md: r.skill_md,
            }))
        })
        .await
    }

    pub async fn skill_draft_project_path(
        &self,
        project_id: i64,
    ) -> Result<Option<String>, PersistError> {
        crate::metrics::time_db("skill_draft_project_path", async move {
            let path = sqlx::query_scalar!(
                "SELECT p.project_path FROM projects.projects p WHERE p.id = $1",
                project_id
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(path)
        })
        .await
    }

    pub async fn save_skill_draft_progress(
        &self,
        id: i64,
        progress: &Value,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("save_skill_draft_progress", async move {
            let done = sqlx::query!(
                "UPDATE memory.skill_drafts
                    SET eval_progress = $2
                  WHERE id = $1 AND status = 'evaluating'",
                id,
                progress
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected() > 0)
        })
        .await
    }

    pub async fn finish_skill_draft_eval(
        &self,
        id: i64,
        accepted: bool,
        eval: &Value,
        patch: Option<ShippedPatch<'_>>,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("finish_skill_draft_eval", async move {
            let status = if accepted { "accepted" } else { "rejected" };
            let description = patch.map(|p| p.description);
            let skill_md = patch.map(|p| p.skill_md);
            let done = sqlx::query!(
                "UPDATE memory.skill_drafts
                    SET status = $2, eval = $3, evaluated_at = now(), eval_progress = NULL,
                        description = COALESCE($4, description),
                        skill_md = COALESCE($5, skill_md)
                  WHERE id = $1 AND status = 'evaluating'",
                id,
                status,
                eval,
                description,
                skill_md
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected() > 0)
        })
        .await
    }

    pub async fn fail_skill_draft_eval(
        &self,
        id: i64,
        error: &str,
        max_attempts: i32,
        evaluator_version: i32,
    ) -> Result<Option<EvalFailure>, PersistError> {
        crate::metrics::time_db("fail_skill_draft_eval", async move {
            let row = sqlx::query!(
                r#"WITH f AS (
                       INSERT INTO memory.skill_draft_eval_failures (draft_id, last_error)
                       VALUES ($1, $2)
                       ON CONFLICT (draft_id) DO UPDATE
                          SET attempts = memory.skill_draft_eval_failures.attempts + 1,
                              last_error = EXCLUDED.last_error,
                              last_failed_at = now()
                       RETURNING attempts)
                   UPDATE memory.skill_drafts d
                      SET status = CASE WHEN f.attempts >= $3 THEN 'rejected' ELSE d.status END,
                          eval = CASE WHEN f.attempts >= $3
                                      THEN jsonb_build_object('evaluator_version', $4::integer, 'error', $2::text, 'attempts', f.attempts)
                                      ELSE d.eval END,
                          evaluated_at = CASE WHEN f.attempts >= $3 THEN now() ELSE d.evaluated_at END,
                          eval_progress = CASE WHEN f.attempts >= $3 THEN NULL ELSE d.eval_progress END
                     FROM f
                    WHERE d.id = $1 AND d.status = 'evaluating'
                RETURNING f.attempts AS "attempts!", d.status AS "status!""#,
                id,
                error,
                max_attempts,
                evaluator_version
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| EvalFailure {
                attempts: r.attempts,
                rejected: r.status == "rejected",
            }))
        })
        .await
    }

    pub async fn requeue_skill_drafts_before_evaluator(
        &self,
        evaluator_version: i32,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("requeue_skill_drafts_before_evaluator", async move {
            let row = sqlx::query!(
                r#"WITH reset AS (
                       UPDATE memory.skill_drafts d
                          SET status = 'draft', eval = NULL, evaluated_at = NULL,
                              eval_progress = NULL
                        WHERE d.status = 'rejected'
                          AND d.evaluated_at IS NOT NULL
                          AND COALESCE((d.eval ->> 'evaluator_version')::integer, 0) < $1
                    RETURNING d.id),
                   cleared AS (
                       DELETE FROM memory.skill_draft_eval_failures f
                        USING reset
                        WHERE f.draft_id = reset.id
                    RETURNING f.draft_id)
                   SELECT count(*) AS "requeued!" FROM reset"#,
                evaluator_version
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(row.requeued)
        })
        .await
    }

    pub async fn recent_skill_listing_bodies(
        &self,
        project_id: i64,
        marker: &[u8],
        window: ListingWindow,
    ) -> Result<Vec<ListingBody>, PersistError> {
        crate::metrics::time_db("recent_skill_listing_bodies", async move {
            let rows = sqlx::query!(
                r#"WITH recent AS (
                       SELECT r.id, r.session_id, r.created_at, r.request_body
                         FROM requests.requests r
                        WHERE r.project_id = $1
                          AND r.request_body_size BETWEEN $5 AND $6
                        ORDER BY r.created_at DESC
                        LIMIT $2),
                   carrying AS (
                       SELECT DISTINCT ON (c.session_id) c.id, c.created_at, c.request_body
                         FROM recent c
                        WHERE position($3::bytea IN c.request_body) > 0
                        ORDER BY c.session_id, c.created_at DESC)
                   SELECT k.id AS "id!", k.request_body AS "body!"
                     FROM carrying k
                    ORDER BY k.created_at DESC
                    LIMIT $4"#,
                project_id,
                window.candidates,
                marker,
                window.sessions,
                window.min_bytes,
                window.max_bytes
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| ListingBody {
                    request_id: r.id,
                    body: r.body,
                })
                .collect())
        })
        .await
    }
}
