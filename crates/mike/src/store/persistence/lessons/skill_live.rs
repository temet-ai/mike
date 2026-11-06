use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::skill_drafts::{DraftTarget, PreviousDraft};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetiredReason {
    Unloaded,
    CorrectedAboveBase,
}

impl RetiredReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unloaded => "unloaded",
            Self::CorrectedAboveBase => "corrected_above_base",
        }
    }

    fn parse(code: &str) -> Option<Self> {
        match code {
            "unloaded" => Some(Self::Unloaded),
            "corrected_above_base" => Some(Self::CorrectedAboveBase),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillLiveOutcome {
    pub project: uuid::Uuid,
    pub folder: String,
    pub first_shipped_on: NaiveDate,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub loads: i32,
    pub load_sessions: i32,
    pub loaded_turns: i32,
    pub judged_turns: i32,
    pub corrected_turns: i32,
    pub base_judged_turns: i32,
    pub base_corrected_turns: i32,
    #[serde(default)]
    pub retired_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub retired_reason: Option<RetiredReason>,
}

impl SkillLiveOutcome {
    pub fn inconsistency(&self) -> Option<String> {
        let counts = [
            ("loads", self.loads),
            ("load_sessions", self.load_sessions),
            ("loaded_turns", self.loaded_turns),
            ("judged_turns", self.judged_turns),
            ("corrected_turns", self.corrected_turns),
            ("base_judged_turns", self.base_judged_turns),
            ("base_corrected_turns", self.base_corrected_turns),
        ];
        if let Some((name, value)) = counts.iter().find(|(_, value)| *value < 0) {
            return Some(format!("{name} is {value}, expected 0 or more"));
        }
        let ordered = [
            (
                "judged_turns",
                self.judged_turns,
                "loaded_turns",
                self.loaded_turns,
            ),
            (
                "corrected_turns",
                self.corrected_turns,
                "judged_turns",
                self.judged_turns,
            ),
            (
                "base_corrected_turns",
                self.base_corrected_turns,
                "base_judged_turns",
                self.base_judged_turns,
            ),
        ];
        if let Some((small, a, large, b)) = ordered.iter().find(|(_, a, _, b)| a > b) {
            return Some(format!("{small} is {a}, more than {large} {b}"));
        }
        (self.retired_at.is_some() != self.retired_reason.is_some())
            .then(|| "retired_at and retired_reason must be given together".to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetiredNumbers {
    pub first_shipped_on: NaiveDate,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub loads: i32,
    pub loaded_turns: i32,
    pub judged_turns: i32,
    pub corrected_turns: i32,
    pub base_judged_turns: i32,
    pub base_corrected_turns: i32,
    pub retired_at: DateTime<Utc>,
    pub reason: RetiredReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetiredSkill {
    pub numbers: RetiredNumbers,
    pub shipped: Option<PreviousDraft>,
}

impl PersistHandle {
    pub async fn upsert_skill_live_outcomes(
        &self,
        outcomes: &[SkillLiveOutcome],
    ) -> Result<u64, PersistError> {
        let payload = serde_json::to_value(outcomes)
            .map_err(|e| PersistError::Database(sqlx::Error::Encode(Box::new(e))))?;
        crate::metrics::time_db("upsert_skill_live_outcomes", async move {
            let done = sqlx::query!(
                r#"WITH incoming AS (
                       SELECT o.*
                         FROM jsonb_to_recordset($1::jsonb) AS o(
                              project uuid, folder text, first_shipped_on date,
                              window_start timestamptz, window_end timestamptz,
                              loads integer, load_sessions integer, loaded_turns integer,
                              judged_turns integer, corrected_turns integer,
                              base_judged_turns integer, base_corrected_turns integer,
                              retired_at timestamptz, retired_reason text))
                   INSERT INTO memory.skill_live_outcomes
                          (project_id, folder, first_shipped_on, window_start, window_end,
                           loads, load_sessions, loaded_turns, judged_turns, corrected_turns,
                           base_judged_turns, base_corrected_turns, retired_at, retired_reason)
                   SELECT p.id, i.folder, i.first_shipped_on, i.window_start, i.window_end,
                          i.loads, i.load_sessions, i.loaded_turns, i.judged_turns,
                          i.corrected_turns, i.base_judged_turns, i.base_corrected_turns,
                          i.retired_at, i.retired_reason
                     FROM incoming i
                     JOIN projects.projects p ON p.project_uuid = i.project
                   ON CONFLICT (project_id, folder, first_shipped_on) DO UPDATE
                      SET window_start = EXCLUDED.window_start,
                          window_end = EXCLUDED.window_end,
                          loads = EXCLUDED.loads,
                          load_sessions = EXCLUDED.load_sessions,
                          loaded_turns = EXCLUDED.loaded_turns,
                          judged_turns = EXCLUDED.judged_turns,
                          corrected_turns = EXCLUDED.corrected_turns,
                          base_judged_turns = EXCLUDED.base_judged_turns,
                          base_corrected_turns = EXCLUDED.base_corrected_turns,
                          retired_at = COALESCE(memory.skill_live_outcomes.retired_at, EXCLUDED.retired_at),
                          retired_reason = COALESCE(memory.skill_live_outcomes.retired_reason, EXCLUDED.retired_reason),
                          measured_at = now()"#,
                payload
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn retired_skill_evidence(
        &self,
        target: &DraftTarget,
    ) -> Result<Option<RetiredSkill>, PersistError> {
        let Some(numbers) = self.latest_retirement(target).await? else {
            return Ok(None);
        };
        let shipped = self.shipped_skill_draft(target).await?;
        Ok(Some(RetiredSkill { numbers, shipped }))
    }

    async fn latest_retirement(
        &self,
        target: &DraftTarget,
    ) -> Result<Option<RetiredNumbers>, PersistError> {
        crate::metrics::time_db("latest_skill_retirement", async move {
            let row = sqlx::query!(
                r#"SELECT o.first_shipped_on, o.window_start, o.window_end, o.loads,
                          o.loaded_turns, o.judged_turns, o.corrected_turns,
                          o.base_judged_turns, o.base_corrected_turns,
                          o.retired_at AS "retired_at!", o.retired_reason AS "reason!"
                     FROM memory.skill_live_outcomes o
                    WHERE o.project_id = $1 AND o.folder = $2
                      AND o.retired_at IS NOT NULL
                      AND NOT EXISTS (
                          SELECT 1 FROM memory.skill_drafts d
                           WHERE d.project_id = o.project_id AND d.folder = o.folder
                             AND d.created_at > o.retired_at AND d.status <> 'rejected')
                    ORDER BY o.first_shipped_on DESC
                    LIMIT 1"#,
                target.project_id,
                target.folder
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.and_then(|r| {
                Some(RetiredNumbers {
                    first_shipped_on: r.first_shipped_on,
                    window_start: r.window_start,
                    window_end: r.window_end,
                    loads: r.loads,
                    loaded_turns: r.loaded_turns,
                    judged_turns: r.judged_turns,
                    corrected_turns: r.corrected_turns,
                    base_judged_turns: r.base_judged_turns,
                    base_corrected_turns: r.base_corrected_turns,
                    retired_at: r.retired_at,
                    reason: RetiredReason::parse(&r.reason)?,
                })
            }))
        })
        .await
    }

    async fn shipped_skill_draft(
        &self,
        target: &DraftTarget,
    ) -> Result<Option<PreviousDraft>, PersistError> {
        crate::metrics::time_db("shipped_skill_draft", async move {
            let row = sqlx::query!(
                r#"SELECT d.revision, d.lesson_watermark, d.description, d.skill_md,
                          d.reference_files, d.eval
                     FROM memory.skill_drafts d
                    WHERE d.project_id = $1 AND d.folder = $2 AND d.status = 'accepted'
                    ORDER BY d.lesson_watermark DESC, d.revision DESC
                    LIMIT 1"#,
                target.project_id,
                target.folder
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| PreviousDraft {
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
