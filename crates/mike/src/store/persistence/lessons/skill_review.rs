use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, Serialize)]
pub struct SkillProjectRow {
    pub project_uuid: Uuid,
    pub project_path: String,
    pub display_name: String,
    pub mode: String,
    pub mode_set: bool,
    pub folders: i64,
    pub shipped: i64,
    pub skill_cap: i32,
    pub waiting: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillFolderRow {
    pub folder: String,
    pub draft_id: Option<i64>,
    pub status: Option<String>,
    pub review: Option<String>,
    pub revision: Option<i32>,
    pub lesson_watermark: Option<i64>,
    pub lesson_count: Option<i32>,
    pub description: Option<String>,
    pub drafted_at: Option<DateTime<Utc>>,
    pub evaluated_at: Option<DateTime<Utc>>,
    pub edited_at: Option<DateTime<Utc>>,
    pub trigger_score: Option<f64>,
    pub patched_trigger_score: Option<f64>,
    pub reviewer: Option<Value>,
    pub verdict_reasons: Option<Value>,
    pub approved_draft_id: Option<i64>,
    pub first_written: Option<NaiveDate>,
    pub live: bool,
    pub installs: Value,
    pub retired_by_user_at: Option<DateTime<Utc>>,
    pub live_use: Option<Value>,
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn list_skill_projects(self) -> Result<Vec<SkillProjectRow>, PersistError> {
        crate::metrics::time_db("list_skill_projects", async move {
            Ok(sqlx::query_as!(
                SkillProjectRow,
                r#"WITH touched AS (
                       SELECT project_id FROM memory.skill_drafts
                       UNION SELECT project_id FROM memory.skill_installs
                       UNION SELECT project_id FROM memory.skill_retirements),
                   latest AS (
                       SELECT DISTINCT ON (d.project_id, d.folder)
                              d.project_id, d.folder, d.id, d.status, d.review
                         FROM memory.skill_drafts d
                        ORDER BY d.project_id, d.folder, d.lesson_watermark DESC, d.revision DESC),
                   approved AS (
                       SELECT DISTINCT ON (d.project_id, d.folder) d.project_id, d.folder, d.id
                         FROM memory.skill_drafts d
                        WHERE d.review = 'approved'
                        ORDER BY d.project_id, d.folder, d.lesson_watermark DESC, d.revision DESC),
                   pending AS (
                       SELECT l.project_id, count(*) AS folders,
                              count(*) FILTER (WHERE l.status = 'accepted' AND l.review IS NULL
                                                AND a.id IS DISTINCT FROM l.id) AS awaiting
                         FROM latest l
                         LEFT JOIN approved a ON a.project_id = l.project_id AND a.folder = l.folder
                        WHERE NOT EXISTS (SELECT 1 FROM memory.skill_retirements r
                                           WHERE r.project_id = l.project_id AND r.folder = l.folder)
                        GROUP BY l.project_id),
                   shipped AS (
                       SELECT i.project_id, count(DISTINCT i.folder) AS shipped
                         FROM memory.skill_installs i
                        WHERE i.retired_at IS NULL
                        GROUP BY i.project_id),
                   cap AS (
                       SELECT CASE WHEN jsonb_typeof(s.value) = 'number'
                                    AND (s.value #>> '{}')::numeric BETWEEN 1 AND 100
                                   THEN (s.value #>> '{}')::numeric::int END AS value
                         FROM config.settings s
                        WHERE s.key = 'lessons.skills.max_per_project')
                   SELECT p.project_uuid AS "project_uuid!", p.project_path AS "project_path!",
                          coalesce(p.custom_name, p.display_name) AS "display_name!",
                          coalesce(sp.mode, 'ask') AS "mode!",
                          sp.project_id IS NOT NULL AS "mode_set!",
                          coalesce(pe.folders, 0) AS "folders!",
                          coalesce(s.shipped, 0) AS "shipped!",
                          coalesce(cap.value, 12) AS "skill_cap!",
                          CASE WHEN coalesce(sp.mode, 'ask') = 'ask'
                               THEN coalesce(pe.awaiting, 0) ELSE 0 END AS "waiting!"
                     FROM touched t
                     JOIN projects.projects p ON p.id = t.project_id
                     LEFT JOIN memory.skill_policies sp ON sp.project_id = p.id
                     LEFT JOIN pending pe ON pe.project_id = p.id
                     LEFT JOIN shipped s ON s.project_id = p.id
                     LEFT JOIN cap ON true
                    ORDER BY p.project_path, p.id"#
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn list_project_skill_folders(
        self,
        project_uuid: Uuid,
    ) -> Result<Vec<SkillFolderRow>, PersistError> {
        crate::metrics::time_db("list_project_skill_folders", async move {
            Ok(sqlx::query_as!(
                SkillFolderRow,
                r#"WITH proj AS (
                       SELECT id FROM projects.projects WHERE project_uuid = $1),
                   folders AS (
                       SELECT d.folder FROM memory.skill_drafts d JOIN proj ON proj.id = d.project_id
                       UNION SELECT i.folder FROM memory.skill_installs i JOIN proj ON proj.id = i.project_id
                       UNION SELECT r.folder FROM memory.skill_retirements r JOIN proj ON proj.id = r.project_id
                       UNION SELECT o.folder FROM memory.skill_live_outcomes o JOIN proj ON proj.id = o.project_id),
                   latest AS (
                       SELECT DISTINCT ON (d.folder) d.folder, d.id, d.status, d.review, d.revision,
                              d.lesson_watermark, cardinality(d.lesson_ids) AS lesson_count,
                              d.description, d.created_at, d.evaluated_at, d.edited_at,
                              (d.eval -> 'trigger' ->> 'score')::float8 AS trigger_score,
                              (d.eval -> 'recheck' ->> 'score')::float8 AS patched_trigger_score,
                              CASE WHEN d.eval ? 'review' THEN jsonb_build_object(
                                  'decision', d.eval #>> '{review,decision}',
                                  'reason', d.eval #>> '{review,reason}',
                                  'overlap', d.eval #>> '{review,overlap}',
                                  'belongs_in', d.eval #>> '{review,belongs_in}') END AS reviewer,
                              d.eval -> 'verdict' -> 'reasons' AS verdict_reasons
                         FROM memory.skill_drafts d JOIN proj ON proj.id = d.project_id
                        ORDER BY d.folder, d.lesson_watermark DESC, d.revision DESC),
                   approved AS (
                       SELECT DISTINCT ON (d.folder) d.folder, d.id
                         FROM memory.skill_drafts d JOIN proj ON proj.id = d.project_id
                        WHERE d.review = 'approved'
                        ORDER BY d.folder, d.lesson_watermark DESC, d.revision DESC),
                   installs AS (
                       SELECT i.folder, min(i.first_written) AS first_written,
                              bool_or(i.retired_at IS NULL) AS live,
                              jsonb_agg(jsonb_build_object(
                                  'machine_name', i.machine_name, 'path', i.skill_path,
                                  'draft_id', i.draft_id, 'first_written', i.first_written,
                                  'retired_at', i.retired_at, 'retired_reason', i.retired_reason,
                                  'reported_at', i.reported_at)
                                  ORDER BY i.machine_name, i.skill_path) AS installs
                         FROM memory.skill_installs i JOIN proj ON proj.id = i.project_id
                        GROUP BY i.folder),
                   live_use AS (
                       SELECT DISTINCT ON (o.folder) o.folder, jsonb_build_object(
                                  'first_shipped_on', o.first_shipped_on,
                                  'window_start', o.window_start, 'window_end', o.window_end,
                                  'loads', o.loads, 'load_sessions', o.load_sessions,
                                  'loaded_turns', o.loaded_turns, 'judged_turns', o.judged_turns,
                                  'corrected_turns', o.corrected_turns,
                                  'base_judged_turns', o.base_judged_turns,
                                  'base_corrected_turns', o.base_corrected_turns,
                                  'retired_at', o.retired_at, 'retired_reason', o.retired_reason,
                                  'measured_at', o.measured_at) AS numbers
                         FROM memory.skill_live_outcomes o JOIN proj ON proj.id = o.project_id
                        ORDER BY o.folder, o.first_shipped_on DESC)
                   SELECT f.folder AS "folder!", l.id AS "draft_id?", l.status AS "status?",
                          l.review AS "review?", l.revision AS "revision?",
                          l.lesson_watermark AS "lesson_watermark?",
                          l.lesson_count AS "lesson_count?", l.description AS "description?",
                          l.created_at AS "drafted_at?", l.evaluated_at AS "evaluated_at?",
                          l.edited_at AS "edited_at?", l.trigger_score AS "trigger_score?",
                          l.patched_trigger_score AS "patched_trigger_score?",
                          l.reviewer AS "reviewer?",
                          l.verdict_reasons AS "verdict_reasons?",
                          a.id AS "approved_draft_id?", i.first_written AS "first_written?",
                          coalesce(i.live, false) AS "live!",
                          coalesce(i.installs, '[]'::jsonb) AS "installs!",
                          r.retired_at AS "retired_by_user_at?",
                          lu.numbers AS "live_use?"
                     FROM folders f
                    CROSS JOIN proj
                     LEFT JOIN latest l ON l.folder = f.folder
                     LEFT JOIN approved a ON a.folder = f.folder
                     LEFT JOIN installs i ON i.folder = f.folder
                     LEFT JOIN live_use lu ON lu.folder = f.folder
                     LEFT JOIN memory.skill_retirements r
                       ON r.project_id = proj.id AND r.folder = f.folder
                    ORDER BY f.folder"#,
                project_uuid
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn skill_project_mode(
        self,
        project_uuid: Uuid,
    ) -> Result<Option<(String, bool)>, PersistError> {
        crate::metrics::time_db("skill_project_mode", async move {
            let row = sqlx::query!(
                r#"SELECT coalesce(sp.mode, 'ask') AS "mode!",
                          sp.project_id IS NOT NULL AS "mode_set!"
                     FROM projects.projects p
                     LEFT JOIN memory.skill_policies sp ON sp.project_id = p.id
                    WHERE p.project_uuid = $1"#,
                project_uuid
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| (r.mode, r.mode_set)))
        })
        .await
    }
}
