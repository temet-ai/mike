use chrono::{DateTime, Utc};
use sqlx::{Postgres, QueryBuilder};

use crate::change_record::{ChangeEvent, ChangeKind, ChangeQuery, NewChangeEvent};
use crate::store::persistence::PersistError;

const PATH_SEPARATOR: &str = "\u{1f}";

#[derive(sqlx::FromRow)]
pub struct ChangeEventRow {
    id: i64,
    project_id: i64,
    checkout: String,
    at: DateTime<Utc>,
    kind: String,
    session_id: Option<i64>,
    run_id: Option<String>,
    task: Option<String>,
    paths: Vec<String>,
    subject: String,
    detail: serde_json::Value,
    before_sha: Option<String>,
    after_sha: Option<String>,
}

fn events_after_builder<'q>(query: &'q ChangeQuery, kinds: &'q [String]) -> QueryBuilder<Postgres> {
    let mut builder = QueryBuilder::new(
        "SELECT e.id, e.project_id, e.checkout, e.at, e.kind, e.session_id, e.run_id, e.task, \
         e.paths, e.subject, e.detail, e.before_sha, e.after_sha \
         FROM workspace.change_events e WHERE e.checkout = ",
    );
    builder
        .push_bind(&query.checkout)
        .push(" AND e.id > ")
        .push_bind(query.after_id);
    if !query.path_prefixes.is_empty() {
        builder
            .push(
                " AND EXISTS (SELECT 1 FROM unnest(e.paths) AS ep(path) \
                 JOIN unnest(",
            )
            .push_bind(&query.path_prefixes[..])
            .push("::text[]) AS wanted(prefix) ON starts_with(ep.path, wanted.prefix))");
    }
    if let Some(excluded) = query.exclude_session {
        builder
            .push(" AND e.session_id IS DISTINCT FROM ")
            .push_bind(excluded);
    }
    if !kinds.is_empty() {
        builder
            .push(" AND e.kind = ANY(")
            .push_bind(kinds)
            .push(")");
    }
    builder.push(" ORDER BY e.id LIMIT ").push_bind(query.limit);
    builder
}

impl TryFrom<ChangeEventRow> for ChangeEvent {
    type Error = PersistError;

    fn try_from(row: ChangeEventRow) -> Result<Self, PersistError> {
        let kind = ChangeKind::parse(&row.kind).ok_or_else(|| {
            PersistError::DataInvariant(format!(
                "change event {} has unknown kind {:?}",
                row.id, row.kind
            ))
        })?;
        Ok(ChangeEvent {
            id: row.id,
            project_id: row.project_id,
            checkout: row.checkout,
            at: row.at,
            kind,
            session_id: row.session_id,
            run_id: row.run_id,
            task: row.task,
            paths: row.paths,
            subject: row.subject,
            detail: row.detail,
            before_sha: row.before_sha,
            after_sha: row.after_sha,
        })
    }
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn insert_change_events(
        self,
        events: &[NewChangeEvent],
    ) -> Result<Vec<i64>, PersistError> {
        if events.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(bad) = events
            .iter()
            .find(|e| e.paths.iter().any(|p| p.contains(PATH_SEPARATOR)))
        {
            return Err(PersistError::DataInvariant(format!(
                "change event for {:?} names a path containing the separator character",
                bad.checkout
            )));
        }
        let project_ids: Vec<i64> = events.iter().map(|e| e.project_id).collect();
        let checkouts: Vec<String> = events.iter().map(|e| e.checkout.clone()).collect();
        let ats: Vec<DateTime<Utc>> = events.iter().map(|e| e.at).collect();
        let kinds: Vec<String> = events.iter().map(|e| e.kind.as_str().to_owned()).collect();
        let sessions: Vec<Option<i64>> = events.iter().map(|e| e.session_id).collect();
        let runs: Vec<Option<String>> = events.iter().map(|e| e.run_id.clone()).collect();
        let tasks: Vec<Option<String>> = events.iter().map(|e| e.task.clone()).collect();
        let paths: Vec<String> = events
            .iter()
            .map(|e| e.paths.join(PATH_SEPARATOR))
            .collect();
        let subjects: Vec<String> = events.iter().map(|e| e.subject.clone()).collect();
        let details: Vec<serde_json::Value> = events.iter().map(|e| e.detail.clone()).collect();
        let before: Vec<Option<String>> = events.iter().map(|e| e.before_sha.clone()).collect();
        let after: Vec<Option<String>> = events.iter().map(|e| e.after_sha.clone()).collect();
        crate::metrics::time_db("change_record_insert_events", async move {
            let rows = sqlx::query_file_scalar!(
                "src/store/persistence/change_record/insert_change_events.sql",
                &project_ids,
                &checkouts,
                &ats,
                &kinds,
                &sessions as &[Option<i64>],
                &runs as &[Option<String>],
                &tasks as &[Option<String>],
                &paths,
                &subjects,
                &details,
                &before as &[Option<String>],
                &after as &[Option<String>],
            )
            .fetch_all(&self.pool)
            .await?;
            self.memo::<super::open_memo::OpenChangesMemo>().revise();
            Ok(rows)
        })
        .await
    }

    pub async fn change_events_after(
        self,
        query: &ChangeQuery,
    ) -> Result<Vec<ChangeEvent>, PersistError> {
        let kinds: Vec<String> = query.kinds.iter().map(|k| k.as_str().to_owned()).collect();
        crate::metrics::time_db("change_record_events_after", async move {
            let rows = events_after_builder(query, &kinds)
                .build_query_as::<ChangeEventRow>()
                .fetch_all(&self.pool)
                .await?;
            rows.into_iter().map(ChangeEvent::try_from).collect()
        })
        .await
    }

    pub async fn latest_change_events(
        self,
        checkout: &str,
        path_prefixes: &[String],
        kinds: &[ChangeKind],
        limit: i64,
    ) -> Result<Vec<ChangeEvent>, PersistError> {
        let kinds: Vec<String> = kinds.iter().map(|k| k.as_str().to_owned()).collect();
        crate::metrics::time_db("change_record_latest_events", async move {
            let rows = if path_prefixes.is_empty() {
                sqlx::query_file_as!(
                    ChangeEventRow,
                    "src/store/persistence/change_record/latest_change_events.sql",
                    checkout,
                    &kinds,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            } else {
                sqlx::query_file_as!(
                    ChangeEventRow,
                    "src/store/persistence/change_record/latest_change_events_prefixed.sql",
                    checkout,
                    path_prefixes,
                    &kinds,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            };
            rows.into_iter().map(ChangeEvent::try_from).collect()
        })
        .await
    }

    pub async fn update_change_author(
        self,
        event_id: i64,
        session: Option<i64>,
        kind: ChangeKind,
        detail: &serde_json::Value,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("change_record_update_author", async move {
            let done = sqlx::query_file!(
                "src/store/persistence/change_record/update_change_author.sql",
                event_id,
                session,
                kind.as_str(),
                detail,
            )
            .execute(&self.pool)
            .await?;
            self.memo::<super::open_memo::OpenChangesMemo>().revise();
            Ok(done.rows_affected() > 0)
        })
        .await
    }
}
