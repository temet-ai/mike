use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillReview {
    Approved,
    Rejected,
    Cleared,
}

impl SkillReview {
    fn code(self) -> Option<&'static str> {
        match self {
            Self::Approved => Some("approved"),
            Self::Rejected => Some("rejected"),
            Self::Cleared => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportedInstall {
    pub project_uuid: Uuid,
    pub folder: String,
    pub path: String,
    pub draft_id: Option<i64>,
    pub lesson_watermark: i64,
    pub first_written: NaiveDate,
    pub retired_at: Option<NaiveDate>,
    pub retired_reason: Option<String>,
}

pub struct InstallReport<'a> {
    pub machine_uuid: Uuid,
    pub machine_name: &'a str,
    pub installs: &'a [ReportedInstall],
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn set_skill_draft_review(
        self,
        id: i64,
        review: SkillReview,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("set_skill_draft_review", async move {
            let result = sqlx::query!(
                "UPDATE memory.skill_drafts
                    SET review = $2,
                        reviewed_at = CASE WHEN $2::text IS NULL THEN NULL ELSE now() END
                  WHERE id = $1",
                id,
                review.code()
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() == 1)
        })
        .await
    }

    pub async fn edit_skill_draft(self, id: i64, skill_md: &str) -> Result<bool, PersistError> {
        crate::metrics::time_db("edit_skill_draft", async move {
            let result = sqlx::query!(
                "UPDATE memory.skill_drafts
                    SET authored_skill_md = coalesce(authored_skill_md, skill_md),
                        skill_md = $2,
                        edited_at = now()
                  WHERE id = $1",
                id,
                skill_md
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() == 1)
        })
        .await
    }

    pub async fn set_skill_policy(
        self,
        project_uuid: Uuid,
        mode: &str,
        changed_by: &str,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("set_skill_policy", async move {
            let result = sqlx::query!(
                "INSERT INTO memory.skill_policies (project_id, mode, changed_by)
                 SELECT p.id, $2, $3 FROM projects.projects p WHERE p.project_uuid = $1
                 ON CONFLICT (project_id) DO UPDATE
                    SET mode = EXCLUDED.mode, changed_at = now(),
                        changed_by = EXCLUDED.changed_by",
                project_uuid,
                mode,
                changed_by
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() == 1)
        })
        .await
    }

    pub async fn retire_skill_folder(
        self,
        project_uuid: Uuid,
        folder: &str,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("retire_skill_folder", async move {
            let result = sqlx::query!(
                "INSERT INTO memory.skill_retirements (project_id, folder)
                 SELECT p.id, $2 FROM projects.projects p WHERE p.project_uuid = $1
                 ON CONFLICT (project_id, folder) DO UPDATE
                    SET retired_at = memory.skill_retirements.retired_at",
                project_uuid,
                folder
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() == 1)
        })
        .await
    }

    pub async fn restore_skill_folder(
        self,
        project_uuid: Uuid,
        folder: &str,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("restore_skill_folder", async move {
            let result = sqlx::query!(
                "DELETE FROM memory.skill_retirements r
                  USING projects.projects p
                  WHERE p.id = r.project_id AND p.project_uuid = $1 AND r.folder = $2",
                project_uuid,
                folder
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() == 1)
        })
        .await
    }

    pub async fn record_skill_installs(
        self,
        report: InstallReport<'_>,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("record_skill_installs", async move {
            let rows = serde_json::to_value(report.installs)?;
            let mut tx = self.pool.begin().await?;
            let written = sqlx::query!(
                r#"INSERT INTO memory.skill_installs AS si
                          (machine_uuid, project_id, folder, machine_name, skill_path, draft_id,
                           lesson_watermark, first_written, retired_at, retired_reason, reported_at)
                   SELECT $1, p.id, r.folder, $2, r.path, d.id, r.lesson_watermark,
                          r.first_written, r.retired_at, r.retired_reason, now()
                     FROM jsonb_to_recordset($3::jsonb)
                          AS r(project_uuid uuid, folder text, path text, draft_id bigint,
                               lesson_watermark bigint, first_written date, retired_at date,
                               retired_reason text)
                     JOIN projects.projects p ON p.project_uuid = r.project_uuid
                     LEFT JOIN memory.skill_drafts d ON d.id = r.draft_id
                   ON CONFLICT (machine_uuid, project_id, folder) DO UPDATE
                      SET machine_name = EXCLUDED.machine_name, skill_path = EXCLUDED.skill_path,
                          draft_id = EXCLUDED.draft_id, lesson_watermark = EXCLUDED.lesson_watermark,
                          first_written = EXCLUDED.first_written, retired_at = EXCLUDED.retired_at,
                          retired_reason = EXCLUDED.retired_reason,
                          reported_at = EXCLUDED.reported_at"#,
                report.machine_uuid,
                report.machine_name,
                rows
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            sqlx::query!(
                r#"DELETE FROM memory.skill_installs si
                    WHERE si.machine_uuid = $1
                      AND NOT EXISTS (
                          SELECT 1
                            FROM jsonb_to_recordset($2::jsonb) AS r(project_uuid uuid, folder text)
                            JOIN projects.projects p ON p.project_uuid = r.project_uuid
                           WHERE p.id = si.project_id AND r.folder = si.folder)"#,
                report.machine_uuid,
                rows
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(written)
        })
        .await
    }
}
