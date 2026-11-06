use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq)]
pub struct CarryNote {
    pub title: String,
    pub data: Value,
    pub created_at: DateTime<Utc>,
    pub session_uuid: Uuid,
    pub session_db_id: i64,
    pub project_id: i64,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarryOutcome {
    Skipped,
    AlreadyCandidate,
    Counted(usize),
    Candidate(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCarry<'a> {
    pub kind: &'a str,
    pub text: &'a str,
    pub discipline: &'a str,
    pub tag: &'a str,
}

async fn carry_exists(
    tx: &mut sqlx::PgConnection,
    project_id: i64,
    new: &NewCarry<'_>,
) -> Result<bool, PersistError> {
    let existing = sqlx::query_file_scalar!(
        "src/store/persistence/fleet/carry_sql/matches.sql",
        project_id,
        new.tag,
        new.discipline,
    )
    .fetch_all(tx)
    .await?;
    Ok(!existing.is_empty())
}

impl PersistHandle {
    pub async fn carry_note_facts(
        &self,
        note_uuid: Uuid,
    ) -> Result<Option<CarryNote>, PersistError> {
        let row = sqlx::query_file!("src/store/persistence/fleet/carry_sql/note.sql", note_uuid)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| CarryNote {
            title: r.title,
            data: r.data,
            created_at: r.created_at,
            session_uuid: r.session_uuid,
            session_db_id: r.session_id,
            project_id: r.project_id,
            role: r.role,
        }))
    }

    pub async fn carry_seat_agent(&self, session: Uuid) -> Result<Option<String>, PersistError> {
        Ok(sqlx::query_file_scalar!(
            "src/store/persistence/fleet/carry_sql/seat_agent.sql",
            session
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn carry_own_candidate(&self, note_uuid: Uuid) -> Result<Option<i64>, PersistError> {
        Ok(
            sqlx::query_file_scalar!("src/store/persistence/fleet/carry_sql/own.sql", note_uuid)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn carry_count_findings(
        &self,
        note: (Uuid, &CarryNote),
        tags: &[String],
        discipline: Option<&str>,
    ) -> Result<usize, PersistError> {
        let (note_uuid, facts) = note;
        let lessons = match discipline {
            Some(discipline) => {
                sqlx::query_file_scalar!(
                    "src/store/persistence/fleet/carry_sql/count_findings_discipline.sql",
                    note_uuid,
                    facts.project_id,
                    tags,
                    facts.created_at,
                    discipline,
                )
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_file_scalar!(
                    "src/store/persistence/fleet/carry_sql/count_findings.sql",
                    note_uuid,
                    facts.project_id,
                    tags,
                    facts.created_at,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(lessons.len())
    }

    pub async fn carry_create_candidate(
        &self,
        note: (Uuid, &CarryNote),
        new: &NewCarry<'_>,
    ) -> Result<Option<i64>, PersistError> {
        let (note_uuid, facts) = note;
        let mut tx = self.pool.begin().await?;
        if carry_exists(&mut tx, facts.project_id, new).await? {
            tx.rollback().await?;
            return Ok(None);
        }
        let lesson = sqlx::query_file_scalar!(
            "src/store/persistence/fleet/carry_sql/create.sql",
            facts.project_id,
            new.kind,
            new.text,
            facts.session_db_id,
            new.discipline,
            new.tag,
            note_uuid,
        )
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(lesson)
    }

    pub async fn carried_for_warmup(
        &self,
        fleet_uuid: Uuid,
        run_id: &str,
        disciplines: &[String],
    ) -> Result<Vec<String>, PersistError> {
        crate::metrics::time_db("launch_carried_for_warmup", async move {
            let lines = sqlx::query_file_scalar!(
                "src/store/persistence/fleet/carry_sql/warm.sql",
                fleet_uuid,
                disciplines,
                run_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(lines)
        })
        .await
    }
}
