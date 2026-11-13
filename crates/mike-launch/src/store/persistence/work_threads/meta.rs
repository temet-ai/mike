use super::lock::{LockedThread, Tx, require_thread};
use super::reads::identity_by_id;
use super::validate::{invalid, non_blank, relative_path, slug, status};
use super::{ThreadIdentity, WorkThreadError};

#[derive(Debug, Clone, Copy, Default)]
pub struct MetaUpdate<'a> {
    pub title: Option<&'a str>,
    pub goal: Option<&'a str>,
    pub status: Option<&'a str>,
    pub branch: Option<&'a str>,
    pub worktree: Option<&'a str>,
    pub depends_on: Option<&'a [String]>,
    pub acceptance_criteria: Option<&'a str>,
}

impl MetaUpdate<'_> {
    pub fn touches_review_rule(&self) -> bool {
        self.status.is_some() || self.acceptance_criteria.is_some()
    }

    fn validate(&self, own_slug: &str) -> Result<(), WorkThreadError> {
        if let Some(value) = self.title {
            non_blank("title", value)?;
        }
        if let Some(value) = self.status {
            status(value)?;
        }
        if let Some(value) = self.worktree.filter(|value| !value.trim().is_empty()) {
            relative_path("worktree", value)?;
        }
        for (index, dependency) in self.depends_on.unwrap_or_default().iter().enumerate() {
            let dependency = slug(dependency)
                .map_err(|error| invalid(format!("depends_on[{index}]: {error}")))?;
            if dependency == own_slug {
                return Err(invalid(format!(
                    "depends_on[{index}] `{dependency}` is the thread itself; a thread cannot depend on itself"
                )));
            }
        }
        Ok(())
    }
}

pub async fn apply_meta(
    tx: &mut Tx<'_>,
    project_id: i64,
    thread: &LockedThread,
    thread_slug: &str,
    meta: &MetaUpdate<'_>,
) -> Result<(), WorkThreadError> {
    meta.validate(thread_slug)?;
    update_identity(tx, thread.id, meta).await?;
    if let Some(text) = meta.acceptance_criteria {
        write_criteria(tx, thread.id, text).await?;
    }
    if let Some(dependencies) = meta.depends_on {
        replace_dependencies(tx, project_id, thread.id, dependencies).await?;
    }
    if meta.touches_review_rule() {
        enforce_review_rule(tx, thread.id, thread_slug).await?;
    }
    Ok(())
}

async fn update_identity(
    tx: &mut Tx<'_>,
    thread_id: i64,
    meta: &MetaUpdate<'_>,
) -> Result<(), WorkThreadError> {
    sqlx::query!(
        "UPDATE work.threads SET
            title    = COALESCE(btrim($2), title),
            status   = COALESCE($3, status),
            goal     = CASE WHEN $4::text IS NULL THEN goal WHEN btrim($4) = '' THEN NULL ELSE btrim($4) END,
            branch   = CASE WHEN $5::text IS NULL THEN branch WHEN btrim($5) = '' THEN NULL ELSE btrim($5) END,
            worktree = CASE WHEN $6::text IS NULL THEN worktree WHEN btrim($6) = '' THEN NULL ELSE btrim($6) END,
            updated_at = now()
          WHERE id = $1",
        thread_id,
        meta.title,
        meta.status.map(str::trim),
        meta.goal,
        meta.branch,
        meta.worktree,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn write_criteria(
    tx: &mut Tx<'_>,
    thread_id: i64,
    text: &str,
) -> Result<(), WorkThreadError> {
    let text = text.trim();
    if text.is_empty() {
        sqlx::query!(
            "DELETE FROM work.thread_acceptance_criteria WHERE thread_id = $1",
            thread_id
        )
        .execute(&mut **tx)
        .await?;
        return Ok(());
    }
    sqlx::query!(
        "INSERT INTO work.thread_acceptance_criteria (thread_id, criteria_text)
         VALUES ($1, $2)
         ON CONFLICT (thread_id) DO UPDATE SET
            criteria_text = EXCLUDED.criteria_text,
            parsed_at = now(),
            parse_error = NULL,
            embedding = NULL, embedding_model = NULL, embedded_at = NULL",
        thread_id,
        text,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn replace_dependencies(
    tx: &mut Tx<'_>,
    project_id: i64,
    thread_id: i64,
    dependencies: &[String],
) -> Result<(), WorkThreadError> {
    let mut wanted: Vec<String> = dependencies.iter().map(|s| s.trim().to_owned()).collect();
    wanted.sort();
    wanted.dedup();
    let missing: Vec<String> = sqlx::query_scalar!(
        r#"SELECT w.slug AS "slug!" FROM unnest($2::text[]) AS w(slug)
          WHERE NOT EXISTS (
                SELECT 1 FROM work.threads t WHERE t.project_id = $1 AND t.slug = w.slug)
          ORDER BY w.slug"#,
        project_id,
        &wanted,
    )
    .fetch_all(&mut **tx)
    .await?;
    if !missing.is_empty() {
        return Err(invalid(format!(
            "depends_on names thread(s) that do not exist in this project: {}",
            missing.join(", ")
        )));
    }
    sqlx::query!(
        "DELETE FROM work.thread_dependencies d
          WHERE d.thread_id = $1
            AND NOT EXISTS (
                SELECT 1 FROM work.threads t
                 WHERE t.id = d.depends_on_thread_id AND t.slug = ANY($2))",
        thread_id,
        &wanted,
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "INSERT INTO work.thread_dependencies (thread_id, depends_on_thread_id)
         SELECT $1::bigint, t.id FROM work.threads t
          WHERE t.project_id = $3 AND t.slug = ANY($2)
         ON CONFLICT DO NOTHING",
        thread_id,
        &wanted,
        project_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn enforce_review_rule(
    tx: &mut Tx<'_>,
    thread_id: i64,
    thread_slug: &str,
) -> Result<(), WorkThreadError> {
    let row = sqlx::query!(
        r#"SELECT t.status, COALESCE(btrim(c.criteria_text), '') AS "criteria!"
           FROM work.threads t
           LEFT JOIN work.thread_acceptance_criteria c ON c.thread_id = t.id
          WHERE t.id = $1"#,
        thread_id,
    )
    .fetch_one(&mut **tx)
    .await?;
    if row.status.as_deref() == Some("review") && row.criteria.is_empty() {
        return Err(invalid(format!(
            "thread `{thread_slug}` cannot be in `review` without acceptance criteria. Send `acceptance_criteria` (the checks that must hold before the thread is done) in the same call"
        )));
    }
    Ok(())
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn update_work_thread_meta(
        self,
        project_id: i64,
        thread_slug: &str,
        meta: &MetaUpdate<'_>,
    ) -> Result<ThreadIdentity, WorkThreadError> {
        let thread_slug = slug(thread_slug)?;
        let mut tx = self.pool().begin().await?;
        let thread = require_thread(&mut tx, project_id, thread_slug).await?;
        apply_meta(&mut tx, project_id, &thread, thread_slug, meta).await?;
        tx.commit().await?;
        Ok(identity_by_id(self.pool(), thread.id).await?)
    }
}
