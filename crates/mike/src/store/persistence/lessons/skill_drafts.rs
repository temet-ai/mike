use serde_json::Value;

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftTarget {
    pub project_id: i64,
    pub project_path: String,
    pub folder: String,
    pub topic: String,
    pub agent_id: Option<i64>,
    pub agent_name: Option<String>,
    pub lesson_watermark: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftLesson {
    pub id: i64,
    pub kind: String,
    pub text: String,
    pub user_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreviousDraft {
    pub revision: i32,
    pub lesson_watermark: i64,
    pub description: String,
    pub skill_md: String,
    pub reference_files: Value,
    pub eval: Option<Value>,
}

pub struct NewSkillDraft<'a> {
    pub target: &'a DraftTarget,
    pub lesson_ids: &'a [i64],
    pub model: &'a str,
    pub description: &'a str,
    pub skill_md: &'a str,
    pub reference_files: &'a Value,
    pub trigger_queries: &'a Value,
    pub generation_id: Option<&'a str>,
    pub cost_usd: Option<f64>,
}

impl PersistHandle {
    pub async fn next_skill_draft_target(&self) -> Result<Option<DraftTarget>, PersistError> {
        crate::metrics::time_db("next_skill_draft_target", async move {
            let row = sqlx::query!(
                r#"WITH topics AS (
                       SELECT l.project_id, l.topic,
                              CASE WHEN count(DISTINCT l.agent_id) = 1
                                        AND bool_and(l.agent_id IS NOT NULL)
                                   THEN min(l.agent_id) END AS agent_id,
                              max(l.id) AS watermark, count(*) AS lesson_count,
                              array_agg(l.id) AS lesson_ids
                         FROM memory.lessons l
                         JOIN memory.lesson_stages ls
                           ON ls.id = l.stage_id AND ls.code IN ('daily', 'long_term')
                         LEFT JOIN memory.lesson_placements lp ON lp.id = l.placement_id
                        WHERE l.archived_at IS NULL
                          AND (l.placement_id IS NULL OR lp.code = 'skill')
                          AND l.topic IS NOT NULL
                          AND l.topic NOT LIKE 'shared:%'
                          AND l.topic NOT LIKE 'verifier:%'
                          AND l.project_id IS NOT NULL
                        GROUP BY l.project_id, l.topic
                       HAVING count(*) >= 2),
                   named AS (
                       SELECT t.project_id, p.project_path, t.topic, t.agent_id,
                              a.name AS agent_name, t.watermark, t.lesson_count, t.lesson_ids,
                              CASE WHEN a.name IS NULL THEN t.topic
                                   ELSE t.topic || '-' || a.name END AS folder
                         FROM topics t
                         JOIN projects.projects p ON p.id = t.project_id
                         LEFT JOIN agents.agents a ON a.id = t.agent_id
                        WHERE p.project_path <> '(no project)')
                   SELECT n.project_id AS "project_id!", n.project_path AS "project_path!",
                          n.topic AS "topic!", n.agent_id, n.agent_name AS "agent_name?",
                          n.folder AS "folder!", n.watermark AS "watermark!"
                     FROM named n
                    WHERE NOT EXISTS (
                          SELECT 1 FROM memory.skill_drafts d
                           WHERE d.project_id = n.project_id
                             AND d.folder = n.folder
                             AND d.lesson_ids @> n.lesson_ids
                             AND (d.status <> 'rejected'
                                  OR COALESCE(d.eval #>> '{review,decision}'
                                              IN ('nothing_to_save', 'wrong_place'), false)
                                  OR d.revision >= 3))
                      AND NOT EXISTS (
                          SELECT 1 FROM memory.skill_draft_failures f
                           WHERE f.project_id = n.project_id
                             AND f.folder = n.folder
                             AND f.lesson_watermark >= n.watermark
                             AND (f.attempts >= 3
                                  OR f.last_failed_at > now() - interval '1 hour' * (2 ^ f.attempts)))
                    ORDER BY n.lesson_count DESC, n.watermark DESC
                    LIMIT 1"#
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| DraftTarget {
                project_id: r.project_id,
                project_path: r.project_path,
                folder: r.folder,
                topic: r.topic,
                agent_id: r.agent_id,
                agent_name: r.agent_name,
                lesson_watermark: r.watermark,
            }))
        })
        .await
    }

    pub async fn skill_draft_lessons(
        &self,
        target: &DraftTarget,
    ) -> Result<Vec<DraftLesson>, PersistError> {
        crate::metrics::time_db("skill_draft_lessons", async move {
            let rows = sqlx::query!(
                r#"SELECT l.id, k.code AS "kind!", l.text, t.user_prompt
                     FROM memory.lessons l
                     JOIN memory.lesson_kinds k ON k.id = l.kind_id
                     JOIN memory.lesson_stages ls
                       ON ls.id = l.stage_id AND ls.code IN ('daily', 'long_term')
                     LEFT JOIN memory.lesson_placements lp ON lp.id = l.placement_id
                     LEFT JOIN memory.operator_turns t ON t.id = l.source_turn_id
                    WHERE l.project_id = $1
                      AND l.topic = $2
                      AND l.archived_at IS NULL
                      AND (l.placement_id IS NULL OR lp.code = 'skill')
                    ORDER BY l.id"#,
                target.project_id,
                target.topic
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

    pub async fn insert_skill_draft(&self, draft: NewSkillDraft<'_>) -> Result<i64, PersistError> {
        crate::metrics::time_db("insert_skill_draft", async move {
            let id = sqlx::query_scalar!(
                "INSERT INTO memory.skill_drafts
                    (project_id, folder, agent_id, lesson_watermark, lesson_ids, model,
                     description, skill_md, reference_files,
                     trigger_queries, generation_id, cost_usd, topic, revision)
                 SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                        count(*)::int + 1
                   FROM memory.skill_drafts
                  WHERE project_id = $1 AND folder = $2 AND lesson_watermark = $4
                 HAVING count(*) < 3
                 RETURNING id",
                draft.target.project_id,
                draft.target.folder,
                draft.target.agent_id,
                draft.target.lesson_watermark,
                draft.lesson_ids,
                draft.model,
                draft.description,
                draft.skill_md,
                draft.reference_files,
                draft.trigger_queries,
                draft.generation_id,
                draft.cost_usd,
                draft.target.topic
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }

    pub async fn record_skill_draft_failure(
        &self,
        target: &DraftTarget,
        error: &str,
    ) -> Result<i32, PersistError> {
        crate::metrics::time_db("record_skill_draft_failure", async move {
            let attempts = sqlx::query_scalar!(
                "INSERT INTO memory.skill_draft_failures
                    (project_id, folder, lesson_watermark, last_error)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (project_id, folder, lesson_watermark) DO UPDATE
                    SET attempts = memory.skill_draft_failures.attempts + 1,
                        last_error = EXCLUDED.last_error,
                        last_failed_at = now()
                 RETURNING attempts",
                target.project_id,
                target.folder,
                target.lesson_watermark,
                error
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(attempts)
        })
        .await
    }

    pub async fn previous_rejected_skill_draft(
        &self,
        target: &DraftTarget,
    ) -> Result<Option<PreviousDraft>, PersistError> {
        crate::metrics::time_db("previous_rejected_skill_draft", async move {
            let row = sqlx::query!(
                r#"SELECT d.revision, d.lesson_watermark, d.status, d.description, d.skill_md,
                          d.reference_files, d.eval
                     FROM memory.skill_drafts d
                    WHERE d.project_id = $1 AND d.folder = $2
                    ORDER BY d.lesson_watermark DESC, d.revision DESC
                    LIMIT 1"#,
                target.project_id,
                target.folder
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row
                .filter(|r| r.status == "rejected")
                .map(|r| PreviousDraft {
                    revision: r.revision,
                    lesson_watermark: r.lesson_watermark,
                    description: r.description,
                    skill_md: r.skill_md,
                    reference_files: r.reference_files,
                    eval: r.eval,
                }))
        })
        .await
    }
}
