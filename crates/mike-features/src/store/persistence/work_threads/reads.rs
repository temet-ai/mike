use sqlx::PgPool;

use super::validate::{slug, status_filter};
use super::{
    HistoryEntry, LinkedSession, ThreadDetail, ThreadIdentity, ThreadList, ThreadListRow,
    ThreadState, WorkThreadError,
};

pub async fn identity_by_id(
    pool: &PgPool,
    thread_id: i64,
) -> Result<ThreadIdentity, sqlx::Error> {
    sqlx::query_as!(
        ThreadIdentity,
        "SELECT id, slug, title, goal, status, branch, worktree, last_session_uuid, source_path, parse_error, created_at, updated_at
           FROM work.threads WHERE id = $1",
        thread_id,
    )
    .fetch_one(pool)
    .await
}

pub async fn identities_by_ids(
    pool: &PgPool,
    thread_ids: &[i64],
) -> Result<Vec<ThreadIdentity>, sqlx::Error> {
    sqlx::query_as!(
        ThreadIdentity,
        "SELECT id, slug, title, goal, status, branch, worktree, last_session_uuid, source_path, parse_error, created_at, updated_at
           FROM work.threads WHERE id = ANY($1) ORDER BY id",
        thread_ids,
    )
    .fetch_all(pool)
    .await
}

pub struct ThreadStateRow {
    pub thread_id: i64,
    pub state: ThreadState,
}

pub async fn states_by_ids(
    pool: &PgPool,
    thread_ids: &[i64],
) -> Result<Vec<ThreadStateRow>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT thread_id, tldr, where_we_are, next_up, blockers, open_questions, decisions, files, updated_at
           FROM work.thread_state WHERE thread_id = ANY($1) ORDER BY thread_id",
        thread_ids,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| ThreadStateRow {
            thread_id: row.thread_id,
            state: ThreadState {
                tldr: row.tldr,
                where_we_are: row.where_we_are,
                next_up: row.next_up,
                blockers: row.blockers,
                open_questions: row.open_questions,
                decisions: row.decisions,
                files: row.files,
                updated_at: row.updated_at,
            },
        })
        .collect())
}

#[derive(Debug, Clone, Copy)]
pub struct HistoryPage {
    pub limit: i64,
    pub before_ord: Option<i32>,
}

impl Default for HistoryPage {
    fn default() -> Self {
        Self {
            limit: 20,
            before_ord: None,
        }
    }
}

impl HistoryPage {
    pub fn everything() -> Self {
        Self {
            limit: i64::MAX - 1,
            before_ord: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ListFilter<'a> {
    pub status: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

impl Default for ListFilter<'_> {
    fn default() -> Self {
        Self {
            status: None,
            limit: 200,
            offset: 0,
        }
    }
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn get_work_thread(
        self,
        project_id: i64,
        thread_slug: &str,
        page: HistoryPage,
    ) -> Result<ThreadDetail, WorkThreadError> {
        let thread_slug = slug(thread_slug)?;
        let thread = sqlx::query_as!(
            ThreadIdentity,
            "SELECT id, slug, title, goal, status, branch, worktree, last_session_uuid, source_path, parse_error, created_at, updated_at
               FROM work.threads WHERE project_id = $1 AND slug = $2",
            project_id,
            thread_slug,
        )
        .fetch_optional(self.pool())
        .await?
            .ok_or_else(|| {
                WorkThreadError::NotFound(format!(
                    "thread `{thread_slug}` does not exist in this project; work_thread_list shows the threads that do"
                ))
            })?;
        let (history, history_total, history_next_before_ord) =
            self.work_thread_history(thread.id, page).await?;
        Ok(ThreadDetail {
            state: self.work_thread_state(thread.id).await?,
            acceptance_criteria: self.work_thread_criteria(thread.id).await?,
            depends_on: self.work_thread_dependencies(thread.id, true).await?,
            depended_on_by: self.work_thread_dependencies(thread.id, false).await?,
            sessions: self.work_thread_sessions(thread.id).await?,
            thread,
            history,
            history_total,
            history_next_before_ord,
        })
    }

    pub async fn list_work_threads(
        self,
        project_id: i64,
        filter: ListFilter<'_>,
    ) -> Result<ThreadList, WorkThreadError> {
        let statuses = filter.status.map(status_filter).transpose()?;
        let (threads, total) = match statuses.as_deref() {
            Some(statuses) => {
                self.list_threads_with_status(project_id, statuses, filter)
                    .await?
            }
            None => self.list_threads_any_status(project_id, filter).await?,
        };
        Ok(ThreadList { threads, total })
    }

    async fn list_threads_with_status(
        self,
        project_id: i64,
        statuses: &[String],
        filter: ListFilter<'_>,
    ) -> Result<(Vec<ThreadListRow>, i64), WorkThreadError> {
        let threads = sqlx::query_as!(
            ThreadListRow,
            r#"SELECT t.slug, t.title, t.status, COALESCE(s.tldr, '') AS "tldr!",
                            COALESCE(s.next_up, '') AS "next_up!", t.parse_error, t.updated_at
                       FROM work.threads t
                       LEFT JOIN work.thread_state s ON s.thread_id = t.id
                      WHERE t.project_id = $1 AND t.status = ANY($2)
                      ORDER BY t.updated_at DESC, t.id DESC
                      LIMIT $3 OFFSET $4"#,
            project_id,
            statuses,
            filter.limit,
            filter.offset,
        )
        .fetch_all(self.pool())
        .await?;
        let total = sqlx::query_scalar!(
            r#"SELECT count(*) AS "total!" FROM work.threads t
                      WHERE t.project_id = $1 AND t.status = ANY($2)"#,
            project_id,
            statuses,
        )
        .fetch_one(self.pool())
        .await?;
        Ok((threads, total))
    }

    async fn list_threads_any_status(
        self,
        project_id: i64,
        filter: ListFilter<'_>,
    ) -> Result<(Vec<ThreadListRow>, i64), WorkThreadError> {
        let threads = sqlx::query_as!(
            ThreadListRow,
            r#"SELECT t.slug, t.title, t.status, COALESCE(s.tldr, '') AS "tldr!",
                            COALESCE(s.next_up, '') AS "next_up!", t.parse_error, t.updated_at
                       FROM work.threads t
                       LEFT JOIN work.thread_state s ON s.thread_id = t.id
                      WHERE t.project_id = $1
                      ORDER BY t.updated_at DESC, t.id DESC
                      LIMIT $2 OFFSET $3"#,
            project_id,
            filter.limit,
            filter.offset,
        )
        .fetch_all(self.pool())
        .await?;
        let total = sqlx::query_scalar!(
            r#"SELECT count(*) AS "total!" FROM work.threads t WHERE t.project_id = $1"#,
            project_id,
        )
        .fetch_one(self.pool())
        .await?;
        Ok((threads, total))
    }

    async fn work_thread_state(
        self,
        thread_id: i64,
    ) -> Result<Option<ThreadState>, WorkThreadError> {
        Ok(sqlx::query_as!(
            ThreadState,
            "SELECT tldr, where_we_are, next_up, blockers, open_questions, decisions, files, updated_at
               FROM work.thread_state WHERE thread_id = $1",
            thread_id,
        )
        .fetch_optional(self.pool())
        .await?)
    }

    async fn work_thread_criteria(self, thread_id: i64) -> Result<Option<String>, WorkThreadError> {
        let text: Option<Option<String>> = sqlx::query_scalar!(
            "SELECT criteria_text FROM work.thread_acceptance_criteria WHERE thread_id = $1",
            thread_id,
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(text.flatten())
    }

    async fn work_thread_dependencies(
        self,
        thread_id: i64,
        forward: bool,
    ) -> Result<Vec<String>, WorkThreadError> {
        if forward {
            return Ok(sqlx::query_scalar!(
                "SELECT t.slug FROM work.thread_dependencies d
                   JOIN work.threads t ON t.id = d.depends_on_thread_id
                  WHERE d.thread_id = $1 ORDER BY t.slug",
                thread_id,
            )
            .fetch_all(self.pool())
            .await?);
        }
        Ok(sqlx::query_scalar!(
            "SELECT t.slug FROM work.thread_dependencies d
               JOIN work.threads t ON t.id = d.thread_id
              WHERE d.depends_on_thread_id = $1 ORDER BY t.slug",
            thread_id,
        )
        .fetch_all(self.pool())
        .await?)
    }

    async fn work_thread_sessions(
        self,
        thread_id: i64,
    ) -> Result<Vec<LinkedSession>, WorkThreadError> {
        Ok(sqlx::query_as!(
            LinkedSession,
            "SELECT session_uuid, contribution, first_seen_at, last_seen_at
               FROM work.thread_sessions WHERE thread_id = $1
              ORDER BY last_seen_at DESC, session_uuid",
            thread_id,
        )
        .fetch_all(self.pool())
        .await?)
    }

    async fn work_thread_history(
        self,
        thread_id: i64,
        page: HistoryPage,
    ) -> Result<(Vec<HistoryEntry>, i64, Option<i32>), WorkThreadError> {
        let mut entries = match page.before_ord {
            Some(before_ord) => {
                sqlx::query_as!(
                    HistoryEntry,
                    "SELECT ord, kind, entry_date, session_uuid, commit_sha, summary, body, data, created_at
                       FROM work.thread_log
                      WHERE thread_id = $1 AND ord < $2
                      ORDER BY ord DESC
                      LIMIT $3",
                    thread_id,
                    before_ord,
                    page.limit.saturating_add(1),
                )
                .fetch_all(self.pool())
                .await?
            }
            None => {
                sqlx::query_as!(
                    HistoryEntry,
                    "SELECT ord, kind, entry_date, session_uuid, commit_sha, summary, body, data, created_at
                       FROM work.thread_log
                      WHERE thread_id = $1
                      ORDER BY ord DESC
                      LIMIT $2",
                    thread_id,
                    page.limit.saturating_add(1),
                )
                .fetch_all(self.pool())
                .await?
            }
        };
        let more = i64::try_from(entries.len()).unwrap_or(i64::MAX) > page.limit;
        entries.truncate(usize::try_from(page.limit).unwrap_or(usize::MAX));
        let next = more
            .then(|| entries.last().map(|entry| entry.ord))
            .flatten();
        let total: i64 = sqlx::query_scalar!(
            r#"SELECT count(*) AS "total!" FROM work.thread_log WHERE thread_id = $1"#,
            thread_id,
        )
        .fetch_one(self.pool())
        .await?;
        Ok((entries, total, next))
    }
}
