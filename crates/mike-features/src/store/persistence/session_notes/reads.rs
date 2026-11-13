use chrono::{DateTime, Utc};
use sqlx::{Postgres, QueryBuilder};

use super::{
    NoteOrder, ProjectNoteFilter, SessionNoteFilter, SessionNoteRow, SessionNoteWithAuthor,
};
use crate::store::persistence::PersistError;

const SESSION_NOTES_SELECT: &str = "SELECT n.note_uuid, n.kind, n.title, n.body, n.status, n.tags, n.data, \
     n.created_at, n.updated_at \
     FROM workspace.session_notes n \
     JOIN sessions.sessions s ON s.id = n.session_id \
     WHERE s.session_uuid = ";

const PROJECT_NOTES_SELECT: &str = "SELECT n.note_uuid, s.session_uuid AS session_uuid, \
     sg.display_name, sg.role, \
     n.kind, n.title, n.body, n.status, n.tags, n.data, n.created_at, n.updated_at \
     FROM workspace.session_notes n \
     JOIN sessions.sessions s ON s.id = n.session_id \
     LEFT JOIN sessions.session_goals sg ON sg.session_id = s.id \
     WHERE s.project_id = ";

const PROJECT_NOTES_COUNT: &str = "SELECT COUNT(*) \
     FROM workspace.session_notes n \
     JOIN sessions.sessions s ON s.id = n.session_id ";

const PROJECT_NOTES_COUNT_GOALS: &str =
    "LEFT JOIN sessions.session_goals sg ON sg.session_id = s.id ";

fn order_sql(order: NoteOrder) -> &'static str {
    match order {
        NoteOrder::NewestFirst => " ORDER BY n.created_at DESC",
        NoteOrder::OldestFirst => " ORDER BY n.created_at ASC",
    }
}

fn push_text_search(query: &mut QueryBuilder<Postgres>, q: Option<&str>) {
    if let Some(q) = q {
        let pattern = format!("%{q}%");
        query
            .push(" AND (n.title ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR n.body ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
}

fn push_note_columns<'q>(
    query: &mut QueryBuilder<Postgres>,
    kind: Option<&'q str>,
    status: Option<&'q str>,
    tag: Option<&'q str>,
) {
    if let Some(kind) = kind {
        query.push(" AND n.kind = ").push_bind(kind);
    }
    if let Some(status) = status {
        query.push(" AND n.status = ").push_bind(status);
    }
    if let Some(tag) = tag {
        query
            .push(" AND n.tags @> ARRAY[")
            .push_bind(tag)
            .push("]::text[]");
    }
}

fn push_project_filters<'q>(query: &mut QueryBuilder<Postgres>, filter: &ProjectNoteFilter<'q>) {
    if let Some(role) = filter.role {
        query
            .push(" AND lower(sg.role) = lower(")
            .push_bind(role)
            .push(")");
    }
    if let Some(name) = filter.name {
        query
            .push(" AND lower(sg.display_name) = lower(")
            .push_bind(name)
            .push(")");
    }
    push_note_columns(query, filter.kind, filter.status, filter.tag);
    if let Some(session_uuid) = filter.session_uuid {
        query.push(" AND s.session_uuid = ").push_bind(session_uuid);
    }
    push_text_search(query, filter.q);
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn session_note_belongs_to(
        self,
        session: uuid::Uuid,
        note: uuid::Uuid,
    ) -> Result<bool, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM workspace.session_notes n
                               JOIN sessions.sessions s ON s.id = n.session_id
                              WHERE n.note_uuid = $1 AND s.session_uuid = $2) AS "belongs!""#,
            note,
            session,
        )
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn fetch_session_notes(
        self,
        session_uuid: uuid::Uuid,
        filter: SessionNoteFilter<'_>,
    ) -> Result<Vec<SessionNoteRow>, PersistError> {
        crate::metrics::time_db("session_notes_fetch_session", async move {
            let mut query = QueryBuilder::<Postgres>::new(SESSION_NOTES_SELECT);
            query.push_bind(session_uuid);
            push_note_columns(&mut query, filter.kind, filter.status, filter.tag);
            push_text_search(&mut query, filter.q);
            query
                .push(order_sql(filter.order))
                .push(" LIMIT ")
                .push_bind(filter.limit)
                .push(" OFFSET ")
                .push_bind(filter.offset);
            let rows = query
                .build_query_as::<SessionNoteRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_project_notes(
        self,
        project_id: i64,
        filter: ProjectNoteFilter<'_>,
    ) -> Result<(i64, Vec<SessionNoteWithAuthor>), PersistError> {
        crate::metrics::time_db("session_notes_fetch_project", async move {
            let total = count_project_notes(&self.pool, project_id, &filter).await?;
            let notes = if total == 0 {
                Vec::new()
            } else {
                page_project_notes(&self.pool, project_id, &filter).await?
            };
            Ok((total, notes))
        })
        .await
    }
}

async fn count_project_notes(
    pool: &sqlx::PgPool,
    project_id: i64,
    filter: &ProjectNoteFilter<'_>,
) -> Result<i64, PersistError> {
    let mut query = QueryBuilder::<Postgres>::new(PROJECT_NOTES_COUNT);
    if filter.role.is_some() || filter.name.is_some() {
        query.push(PROJECT_NOTES_COUNT_GOALS);
    }
    query.push("WHERE s.project_id = ").push_bind(project_id);
    push_project_filters(&mut query, filter);
    let count: i64 = query.build_query_scalar().fetch_one(pool).await?;
    Ok(count)
}

#[derive(sqlx::FromRow)]
struct ProjectNoteRow {
    note_uuid: uuid::Uuid,
    session_uuid: uuid::Uuid,
    display_name: Option<String>,
    role: Option<String>,
    kind: String,
    title: String,
    body: String,
    status: Option<String>,
    tags: Vec<String>,
    data: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

async fn page_project_notes(
    pool: &sqlx::PgPool,
    project_id: i64,
    filter: &ProjectNoteFilter<'_>,
) -> Result<Vec<SessionNoteWithAuthor>, PersistError> {
    let mut query = QueryBuilder::<Postgres>::new(PROJECT_NOTES_SELECT);
    query.push_bind(project_id);
    push_project_filters(&mut query, filter);
    query
        .push(order_sql(filter.order))
        .push(" LIMIT ")
        .push_bind(filter.limit)
        .push(" OFFSET ")
        .push_bind(filter.offset);
    let rows = query
        .build_query_as::<ProjectNoteRow>()
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| SessionNoteWithAuthor {
            note_uuid: r.note_uuid,
            session_uuid: r.session_uuid,
            display_name: r.display_name,
            role: r.role,
            kind: r.kind,
            title: r.title,
            body: r.body,
            status: r.status,
            tags: r.tags,
            data: r.data,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect())
}
