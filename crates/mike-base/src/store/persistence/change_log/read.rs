use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{Postgres, QueryBuilder};

use super::super::{PersistError, PersistHandle};
use super::{CHANGE_LOG_COLUMNS, ChangeLogRow};

#[derive(Debug, Clone, Default)]
pub struct ChangeLogFilter {
    pub subject_kind: Option<String>,
    pub subject_key: Option<String>,
    pub subject_key_prefix: Option<String>,
    pub via: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub before_id: Option<i64>,
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct SnapshotEntry {
    pub subject_kind: String,
    pub subject_key: String,
    pub value: Option<serde_json::Value>,
    pub source: String,
    pub redacted: bool,
    pub changed_at: Option<DateTime<Utc>>,
    pub change_id: Option<i64>,
    pub via: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct SnapshotBuild {
    pub change_id: i64,
    pub sha: Option<String>,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConfigSnapshot {
    pub at: DateTime<Utc>,
    pub baseline_at: Option<DateTime<Utc>>,
    pub build: Option<SnapshotBuild>,
    pub entries: Vec<SnapshotEntry>,
}

const SNAPSHOT_KINDS: [&str; 3] = ["setting", "toggle", "steerer"];

fn push_change_filter<'a>(
    query: &mut QueryBuilder<Postgres>,
    filter: &'a ChangeLogFilter,
    prefix: Option<&'a str>,
) {
    query.push(" WHERE TRUE");
    if let Some(kind) = filter.subject_kind.as_deref() {
        query.push(" AND subject_kind = ").push_bind(kind);
    }
    if let Some(key) = filter.subject_key.as_deref() {
        query.push(" AND subject_key = ").push_bind(key);
    }
    if let Some(prefix) = prefix {
        query
            .push(" AND subject_key LIKE ")
            .push_bind(prefix)
            .push(" ESCAPE '\\'");
    }
    if let Some(via) = filter.via.as_deref() {
        query.push(" AND via = ").push_bind(via);
    }
    if let Some(since) = filter.since {
        query.push(" AND changed_at >= ").push_bind(since);
    }
    if let Some(until) = filter.until {
        query.push(" AND changed_at <= ").push_bind(until);
    }
    if let Some(before_id) = filter.before_id {
        query.push(" AND id < ").push_bind(before_id);
    }
}

pub fn like_prefix(prefix: &str) -> String {
    let mut out = String::with_capacity(prefix.len() + 1);
    for ch in prefix.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('%');
    out
}

impl PersistHandle {
    pub async fn list_config_changes(
        &self,
        filter: &ChangeLogFilter,
    ) -> Result<Vec<ChangeLogRow>, PersistError> {
        let prefix = filter.subject_key_prefix.as_deref().map(like_prefix);
        crate::metrics::time_db("list_config_changes", async move {
            let mut query = QueryBuilder::<Postgres>::new("SELECT ");
            query
                .push(CHANGE_LOG_COLUMNS)
                .push(" FROM config.change_log");
            push_change_filter(&mut query, filter, prefix.as_deref());
            query
                .push(" ORDER BY id DESC LIMIT ")
                .push_bind(filter.limit);
            let rows = query
                .build_query_as::<ChangeLogRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn config_snapshot_at(
        &self,
        at: DateTime<Utc>,
        subject_kind: Option<&str>,
    ) -> Result<ConfigSnapshot, PersistError> {
        crate::metrics::time_db("config_snapshot_at", async move {
            let baseline_at = sqlx::query_scalar!(
                "SELECT min(changed_at) FROM config.change_log
  WHERE op = 'observe' AND subject_kind <> 'build' AND detail->>'baseline' = 'true'"
            )
            .fetch_one(&self.pool)
            .await?;
            let build = sqlx::query_as!(
                SnapshotBuild,
                r#"SELECT id AS "change_id!", new_value->>'sha' AS sha, changed_at AS "changed_at!"
   FROM config.change_log
  WHERE subject_kind = 'build' AND changed_at <= $1
  ORDER BY id DESC
  LIMIT 1"#,
                at,
            )
            .fetch_optional(&self.pool)
            .await?;
            let kinds: Vec<&str> = match subject_kind {
                Some(kind) => vec![kind],
                None => SNAPSHOT_KINDS.to_vec(),
            };
            let entries = sqlx::query_file_as!(
                SnapshotEntry,
                "src/store/persistence/change_log/snapshot.sql",
                at,
                &kinds as &[&str],
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(ConfigSnapshot {
                at,
                baseline_at,
                build,
                entries,
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::like_prefix;

    #[test]
    fn like_prefix_escapes_wildcards() {
        assert_eq!(like_prefix("judge_worker."), "judge\\_worker.%");
        assert_eq!(like_prefix("100%"), "100\\%%");
        assert_eq!(like_prefix("a\\b"), "a\\\\b%");
    }
}
