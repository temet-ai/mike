use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Serialize)]
pub struct SkillDraftDetail {
    pub id: i64,
    pub project_id: i64,
    pub project_uuid: Uuid,
    pub project_path: String,
    pub folder: String,
    pub agent_name: Option<String>,
    pub status: String,
    pub review: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub edited_at: Option<DateTime<Utc>>,
    pub revision: i32,
    pub lesson_watermark: i64,
    pub lesson_ids: Vec<i64>,
    pub model: String,
    pub description: String,
    pub skill_md: String,
    pub authored_skill_md: Option<String>,
    pub reference_files: Value,
    pub trigger_queries: Value,
    pub eval: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub evaluated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillDraftLesson {
    pub id: i64,
    pub kind: String,
    pub stage: String,
    pub text: String,
    pub topic: Option<String>,
    pub archived_at: Option<DateTime<Utc>>,
    pub user_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillDraftRevision {
    pub id: i64,
    pub revision: i32,
    pub lesson_watermark: i64,
    pub status: String,
    pub review: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl PersistHandle {
    pub async fn skill_draft_detail(
        &self,
        id: i64,
    ) -> Result<Option<SkillDraftDetail>, PersistError> {
        crate::metrics::time_db("skill_draft_detail", async move {
            Ok(sqlx::query_as!(
                SkillDraftDetail,
                r#"SELECT d.id, d.project_id, p.project_uuid, p.project_path, d.folder,
                          a.name AS "agent_name?", d.status, d.review, d.reviewed_at, d.edited_at,
                          d.revision, d.lesson_watermark, d.lesson_ids, d.model, d.description,
                          d.skill_md, d.authored_skill_md, d.reference_files,
                          d.trigger_queries, d.eval, d.created_at, d.evaluated_at
                     FROM memory.skill_drafts d
                     JOIN projects.projects p ON p.id = d.project_id
                     LEFT JOIN agents.agents a ON a.id = d.agent_id
                    WHERE d.id = $1"#,
                id
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn skill_draft_source_lessons(
        &self,
        lesson_ids: &[i64],
    ) -> Result<Vec<SkillDraftLesson>, PersistError> {
        crate::metrics::time_db("skill_draft_source_lessons", async move {
            Ok(sqlx::query_as!(
                SkillDraftLesson,
                r#"SELECT l.id, k.code AS "kind!", st.code AS "stage!", l.text, l.topic,
                          l.archived_at, t.user_prompt AS "user_prompt?"
                     FROM memory.lessons l
                     JOIN memory.lesson_kinds k ON k.id = l.kind_id
                     JOIN memory.lesson_stages st ON st.id = l.stage_id
                     LEFT JOIN memory.operator_turns t ON t.id = l.source_turn_id
                    WHERE l.id = ANY($1)
                    ORDER BY l.id"#,
                lesson_ids
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn skill_draft_revisions(
        &self,
        project_id: i64,
        folder: &str,
    ) -> Result<Vec<SkillDraftRevision>, PersistError> {
        crate::metrics::time_db("skill_draft_revisions", async move {
            Ok(sqlx::query_as!(
                SkillDraftRevision,
                r#"SELECT d.id, d.revision, d.lesson_watermark, d.status, d.review, d.created_at
                     FROM memory.skill_drafts d
                    WHERE d.project_id = $1 AND d.folder = $2
                    ORDER BY d.lesson_watermark DESC, d.revision DESC"#,
                project_id,
                folder
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
