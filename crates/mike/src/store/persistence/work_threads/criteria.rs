use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ThreadCriteria {
    pub(crate) thread_slug: String,
    pub(crate) criteria_text: String,
    pub(crate) via: CriteriaSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CriteriaSource {
    LinkedSession,
    MentionedInAsk,
}

struct SlugCriteria {
    slug: String,
    criteria_text: String,
}

fn is_slug_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

fn continues_slug(rest: &str) -> bool {
    let mut chars = rest.chars();
    match chars.next() {
        Some('.') => chars.next().is_some_and(is_slug_char),
        Some(c) => is_slug_char(c),
        None => false,
    }
}

fn contains_ending_at_slug_boundary(text: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(offset) = text[from..].find(needle) {
        let end = from + offset + needle.len();
        if !continues_slug(&text[end..]) {
            return true;
        }
        from = end;
    }
    false
}

pub(super) fn ask_mentions_thread(text: &str, slug: &str) -> bool {
    if text.is_empty() || slug.is_empty() {
        return false;
    }
    [
        format!("thread: {slug}"),
        format!("thread {slug}"),
        format!("#{slug}"),
        format!("artifacts/threads/{slug}"),
    ]
    .iter()
    .any(|needle| contains_ending_at_slug_boundary(text, needle))
}

fn first_mentioned(text: &str, threads: Vec<SlugCriteria>) -> Option<SlugCriteria> {
    threads
        .into_iter()
        .find(|thread| ask_mentions_thread(text, &thread.slug))
}

fn non_empty(text: Option<String>) -> Option<String> {
    text.filter(|t| !t.trim().is_empty())
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub(crate) async fn fetch_thread_criteria_for_session(
        self,
        session_db_id: i64,
        accumulated_ask: &str,
    ) -> Result<Option<ThreadCriteria>, PersistError> {
        crate::metrics::time_db("fetch_thread_criteria_for_session", async move {
            if let Some(linked) = self.linked_thread_criteria(session_db_id).await? {
                return Ok(Some(linked));
            }
            if accumulated_ask.is_empty() {
                return Ok(None);
            }
            let project_threads = self.project_threads_with_criteria(session_db_id).await?;
            Ok(
                first_mentioned(accumulated_ask, project_threads).map(|thread| ThreadCriteria {
                    thread_slug: thread.slug,
                    criteria_text: thread.criteria_text,
                    via: CriteriaSource::MentionedInAsk,
                }),
            )
        })
        .await
    }

    async fn linked_thread_criteria(
        self,
        session_db_id: i64,
    ) -> Result<Option<ThreadCriteria>, PersistError> {
        let row = sqlx::query!(
            "SELECT t.slug, c.criteria_text
               FROM sessions.sessions s
               JOIN work.thread_sessions ts ON ts.session_uuid = s.session_uuid
               JOIN work.threads t ON t.id = ts.thread_id
               LEFT JOIN work.thread_acceptance_criteria c ON c.thread_id = t.id
              WHERE s.id = $1
              ORDER BY ts.last_seen_at DESC, t.id DESC
              LIMIT 1",
            session_db_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(|r| {
            non_empty(r.criteria_text).map(|criteria_text| ThreadCriteria {
                thread_slug: r.slug,
                criteria_text,
                via: CriteriaSource::LinkedSession,
            })
        }))
    }

    async fn project_threads_with_criteria(
        self,
        session_db_id: i64,
    ) -> Result<Vec<SlugCriteria>, PersistError> {
        let rows = sqlx::query!(
            "SELECT t.slug, c.criteria_text AS \"criteria_text!\"
               FROM work.threads t
               JOIN work.thread_acceptance_criteria c ON c.thread_id = t.id
              WHERE t.project_id = (SELECT project_id FROM sessions.sessions WHERE id = $1)
                AND btrim(COALESCE(c.criteria_text, '')) <> ''
              ORDER BY t.updated_at DESC, t.slug",
            session_db_id,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| SlugCriteria {
                slug: r.slug,
                criteria_text: r.criteria_text,
            })
            .collect())
    }
}
