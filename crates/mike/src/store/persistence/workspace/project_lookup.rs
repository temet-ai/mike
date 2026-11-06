use chrono::{DateTime, Utc};

use super::ProjectPathResolution;
use crate::extract::request::{compute_project_root, legacy_trimmed_alias};
use crate::store::persistence::id_cache::CachedProject;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathMatch {
    Exact,
    OrSuffix,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct PathRow {
    pub(super) id: i64,
    pub(super) project_path: String,
    pub(super) rank: Option<i32>,
    pub(super) last_seen: Option<DateTime<Utc>>,
}

pub(super) fn path_candidates(path: &str) -> Vec<String> {
    let mut candidates = vec![path.to_owned()];
    if !path.starts_with('/') {
        return candidates;
    }
    let root = compute_project_root(path);
    if root != path {
        candidates.push(root.to_owned());
    }
    if let Some(alias) = legacy_trimmed_alias(root) {
        candidates.push(alias);
    }
    candidates
}

fn is_alias_of(candidate: &str, path: &str) -> bool {
    candidate.starts_with('/')
        && candidate.ends_with(&format!("/{path}"))
        && legacy_trimmed_alias(candidate).as_deref() == Some(path)
}

pub(super) fn decide(
    path: &str,
    matching: PathMatch,
    rows: Vec<PathRow>,
) -> (ProjectPathResolution, bool) {
    let relative = !path.starts_with('/');
    let (ranked, suffixed): (Vec<PathRow>, Vec<PathRow>) =
        rows.into_iter().partition(|row| row.rank.is_some());
    let alias = relative
        .then(|| {
            suffixed
                .iter()
                .filter(|row| is_alias_of(&row.project_path, path))
                .max_by_key(|row| row.last_seen)
        })
        .flatten();
    let best = alias
        .map(|row| row.id)
        .or_else(|| ranked.iter().min_by_key(|row| row.rank).map(|row| row.id));
    if let Some(id) = best {
        return (ProjectPathResolution::Resolved(id, path.to_owned()), false);
    }
    if matching == PathMatch::Exact {
        return (ProjectPathResolution::NotFound, false);
    }
    let mut suffixed = suffixed;
    match suffixed.len() {
        0 => (ProjectPathResolution::NotFound, false),
        1 => {
            let row = suffixed.remove(0);
            (
                ProjectPathResolution::Resolved(row.id, row.project_path),
                true,
            )
        }
        _ => (
            ProjectPathResolution::Ambiguous(
                suffixed.into_iter().map(|row| row.project_path).collect(),
            ),
            false,
        ),
    }
}

impl PersistHandle {
    pub async fn find_project_id_by_path(
        &self,
        project_path: &str,
    ) -> Result<Option<i64>, PersistError> {
        match self.resolve_project(project_path, PathMatch::Exact).await? {
            ProjectPathResolution::Resolved(id, _) => Ok(Some(id)),
            ProjectPathResolution::Ambiguous(_) | ProjectPathResolution::NotFound => Ok(None),
        }
    }

    pub(crate) async fn resolve_project(
        &self,
        project_path: &str,
        matching: PathMatch,
    ) -> Result<ProjectPathResolution, PersistError> {
        if let Some(hit) = self.id_cache.project(project_path)
            && (matching == PathMatch::OrSuffix || !hit.via_suffix)
        {
            return Ok(ProjectPathResolution::Resolved(hit.id, hit.path));
        }
        let rows = self.fetch_path_rows(project_path, matching).await?;
        let (resolution, via_suffix) = decide(project_path, matching, rows);
        if let ProjectPathResolution::Resolved(id, path) = &resolution {
            self.id_cache.remember_project(
                project_path,
                CachedProject {
                    id: *id,
                    path: path.clone(),
                    via_suffix,
                },
            );
        }
        Ok(resolution)
    }

    async fn fetch_path_rows(
        &self,
        project_path: &str,
        matching: PathMatch,
    ) -> Result<Vec<PathRow>, PersistError> {
        let candidates = path_candidates(project_path);
        let relative = !project_path.starts_with('/');
        let or_suffix = matching == PathMatch::OrSuffix;
        let project_path = project_path.to_owned();
        crate::metrics::time_db("workspace_resolve_project", async move {
            let rows = sqlx::query_as!(
                PathRow,
                r#"WITH hit AS (
                     SELECT id, project_path, array_position($2::text[], project_path) AS rank
                       FROM projects.projects
                      WHERE project_path = ANY($2::text[])
                 )
                 SELECT id AS "id!", project_path AS "project_path!", rank AS "rank?",
                        NULL::timestamptz AS "last_seen?"
                   FROM hit
                 UNION ALL
                 SELECT id, project_path, NULL::int, last_seen
                   FROM projects.projects
                  WHERE ($3::boolean OR ($4::boolean AND NOT EXISTS (SELECT 1 FROM hit)))
                    AND project_path LIKE '%' || $1::text"#,
                project_path,
                &candidates,
                relative,
                or_suffix
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub(crate) async fn list_recent_project_paths(
        &self,
        limit: i64,
    ) -> Result<Vec<String>, PersistError> {
        crate::metrics::time_db("workspace_list_recent_project_paths", async move {
            let paths = sqlx::query_scalar!(
                "SELECT project_path FROM projects.projects ORDER BY last_seen DESC LIMIT $1",
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(paths)
        })
        .await
    }
}

#[cfg(test)]
#[path = "project_lookup_tests.rs"]
mod project_lookup_tests;
