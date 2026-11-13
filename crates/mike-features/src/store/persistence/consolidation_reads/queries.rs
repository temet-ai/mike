use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use super::{
    ACTION_FROM, ACTION_SELECT, ArchivedLessonRow, ConsolidationActionRow,
    ConsolidationDecisionsFilter, ConsolidationRunRow, ConsolidationRunsFilter, LessonHistory,
    LessonHistoryHeaderRow, LessonInjectionRow, RUN_FROM, RUN_SELECT, RestoreLessonOutcome,
    RestoredLessonRow,
};
use crate::store::persistence::PersistError;

fn push_run_filter(query: &mut QueryBuilder<Postgres>, filter: &ConsolidationRunsFilter) {
    query.push(" WHERE TRUE");
    if let Some(kind) = &filter.kind {
        query.push(" AND rk.code = ").push_bind(kind);
    }
    if let Some(status) = &filter.status {
        query.push(" AND st.code = ").push_bind(status);
    }
    if filter.project.is_none() && filter.session.is_none() && filter.file.is_none() {
        return;
    }
    query.push(
        " AND EXISTS (SELECT 1
                        FROM memory.consolidation_actions af
                        LEFT JOIN projects.projects afp ON afp.id = af.project_id
                       WHERE af.run_id = r.id",
    );
    if let Some(project) = &filter.project {
        query.push(" AND afp.project_path = ").push_bind(project);
    }
    if let Some(session) = filter.session {
        query.push(" AND af.session_uuid = ").push_bind(session);
    }
    if let Some(file) = &filter.file {
        query
            .push(" AND af.files_touched && ARRAY[")
            .push_bind(file)
            .push("::text]");
    }
    query.push(")");
}

fn push_decision_filter(query: &mut QueryBuilder<Postgres>, filter: &ConsolidationDecisionsFilter) {
    query.push(" WHERE TRUE");
    if let Some(project) = &filter.project {
        query.push(" AND p.project_path = ").push_bind(project);
    }
    if let Some(session) = filter.session {
        query.push(" AND a.session_uuid = ").push_bind(session);
    }
    if let Some(file) = &filter.file {
        query
            .push(" AND a.files_touched && ARRAY[")
            .push_bind(file)
            .push("::text]");
    }
    if let Some(action_kind) = &filter.action_kind {
        query.push(" AND ak.code = ").push_bind(action_kind);
    }
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn list_consolidation_runs(
        self,
        filter: &ConsolidationRunsFilter,
    ) -> Result<(i64, Vec<ConsolidationRunRow>), PersistError> {
        crate::metrics::time_db("list_consolidation_runs", async move {
            let mut count = QueryBuilder::<Postgres>::new(
                "SELECT COUNT(*)
                   FROM memory.consolidation_runs r
                   JOIN memory.consolidation_run_kinds rk ON rk.id = r.run_kind_id
                   JOIN memory.consolidation_run_statuses st ON st.id = r.status_id",
            );
            push_run_filter(&mut count, filter);
            let total_count: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

            let mut page = QueryBuilder::<Postgres>::new("SELECT ");
            page.push(RUN_SELECT).push(" FROM ").push(RUN_FROM);
            push_run_filter(&mut page, filter);
            page.push(" ORDER BY r.started_at DESC OFFSET ")
                .push_bind(filter.offset)
                .push(" LIMIT ")
                .push_bind(filter.limit);
            let rows = page
                .build_query_as::<ConsolidationRunRow>()
                .fetch_all(&self.pool)
                .await?;

            Ok((total_count, rows))
        })
        .await
    }

    pub async fn get_consolidation_run(
        self,
        run_id: i64,
    ) -> Result<Option<(ConsolidationRunRow, Vec<ConsolidationActionRow>)>, PersistError> {
        crate::metrics::time_db("get_consolidation_run", async move {
            let run = sqlx::query_file_as!(
                ConsolidationRunRow,
                "src/store/persistence/consolidation_reads/run_by_id.sql",
                run_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            let Some(run) = run else {
                return Ok(None);
            };

            let actions = sqlx::query_file_as!(
                ConsolidationActionRow,
                "src/store/persistence/consolidation_reads/actions_by_run.sql",
                run_id,
            )
            .fetch_all(&self.pool)
            .await?;

            Ok(Some((run, actions)))
        })
        .await
    }

    pub async fn list_consolidation_decisions(
        self,
        filter: &ConsolidationDecisionsFilter,
    ) -> Result<(i64, Vec<ConsolidationActionRow>), PersistError> {
        crate::metrics::time_db("list_consolidation_decisions", async move {
            let mut count = QueryBuilder::<Postgres>::new(
                "SELECT COUNT(*)
                   FROM memory.consolidation_actions a
                   LEFT JOIN projects.projects p ON p.id = a.project_id
                   JOIN memory.consolidation_action_kinds ak ON ak.id = a.action_kind_id",
            );
            push_decision_filter(&mut count, filter);
            let total_count: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

            let mut page = QueryBuilder::<Postgres>::new("SELECT ");
            page.push(ACTION_SELECT).push(" FROM ").push(ACTION_FROM);
            push_decision_filter(&mut page, filter);
            page.push(" ORDER BY a.created_at DESC OFFSET ")
                .push_bind(filter.offset)
                .push(" LIMIT ")
                .push_bind(filter.limit);
            let rows = page
                .build_query_as::<ConsolidationActionRow>()
                .fetch_all(&self.pool)
                .await?;

            Ok((total_count, rows))
        })
        .await
    }

    pub async fn get_lesson_history(
        self,
        lesson_id: i64,
    ) -> Result<Option<LessonHistory>, PersistError> {
        crate::metrics::time_db("get_lesson_history", async move {
            let header = sqlx::query_file_as!(
                LessonHistoryHeaderRow,
                "src/store/persistence/consolidation_reads/lesson_history_header.sql",
                lesson_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            let Some(header) = header else {
                return Ok(None);
            };

            let actions = sqlx::query_file_as!(
                ConsolidationActionRow,
                "src/store/persistence/consolidation_reads/actions_by_lesson.sql",
                lesson_id,
            )
            .fetch_all(&self.pool)
            .await?;

            let injections = sqlx::query_file_as!(
                LessonInjectionRow,
                "src/store/persistence/consolidation_reads/lesson_injections.sql",
                lesson_id,
            )
            .fetch_all(&self.pool)
            .await?;

            Ok(Some(LessonHistory {
                header,
                actions,
                injections,
            }))
        })
        .await
    }

    pub async fn list_session_memory_actions(
        self,
        session_uuid: Uuid,
    ) -> Result<Option<Vec<ConsolidationActionRow>>, PersistError> {
        crate::metrics::time_db("list_session_memory_actions", async move {
            let Some(session_id) = self.session_row_id(session_uuid).await? else {
                return Ok(None);
            };

            let rows = sqlx::query_file_as!(
                ConsolidationActionRow,
                "src/store/persistence/consolidation_reads/actions_by_session.sql",
                session_id,
            )
            .fetch_all(&self.pool)
            .await?;

            Ok(Some(rows))
        })
        .await
    }

    pub async fn restore_lesson(
        self,
        id: i64,
    ) -> Result<RestoreLessonOutcome, PersistError> {
        crate::metrics::time_db("restore_lesson", async move {
            let restored_id = sqlx::query_file_scalar!(
                "src/store/persistence/consolidation_reads/restore_lesson.sql",
                id,
            )
            .fetch_optional(&self.pool)
            .await?;

            let Some(restored_id) = restored_id else {
                let exists = sqlx::query_scalar!(
                    r#"SELECT EXISTS(SELECT 1 FROM memory.lessons WHERE id = $1) AS "exists!""#,
                    id,
                )
                .fetch_one(&self.pool)
                .await?;
                return Ok(if exists {
                    RestoreLessonOutcome::NotArchived
                } else {
                    RestoreLessonOutcome::NotFound
                });
            };

            let row = sqlx::query_file_as!(
                RestoredLessonRow,
                "src/store/persistence/consolidation_reads/restored_lesson.sql",
                restored_id,
            )
            .fetch_one(&self.pool)
            .await?;

            Ok(RestoreLessonOutcome::Restored(row.into()))
        })
        .await
    }

    pub async fn list_archived_lessons(
        self,
        project_path: &str,
    ) -> Result<Vec<ArchivedLessonRow>, PersistError> {
        crate::metrics::time_db("list_archived_lessons", async move {
            let rows = sqlx::query_file_as!(
                ArchivedLessonRow,
                "src/store/persistence/consolidation_reads/archived_lessons.sql",
                project_path,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
