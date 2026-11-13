use crate::store::persistence::PersistError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewHarvestRun {
    pub project_id: i64,
    pub session_id: Option<i64>,
    pub request_id: i64,
    pub tags_found: usize,
    pub lessons_written: usize,
    pub lessons_skipped: usize,
    pub lessons_truncated: usize,
}

fn count_column(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn record_harvest_run(self, run: NewHarvestRun) -> Result<(), PersistError> {
        crate::metrics::time_db("record_harvest_run", async move {
            sqlx::query!(
                "INSERT INTO memory.harvest_runs
                    (project_id, session_id, request_id, tags_found,
                     lessons_written, lessons_skipped, lessons_truncated)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
                run.project_id,
                run.session_id,
                run.request_id,
                count_column(run.tags_found),
                count_column(run.lessons_written),
                count_column(run.lessons_skipped),
                count_column(run.lessons_truncated),
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::count_column;

    #[test]
    fn count_column_saturates_instead_of_wrapping() {
        assert_eq!(count_column(3), 3);
        assert_eq!(count_column(usize::MAX), i32::MAX);
    }
}
