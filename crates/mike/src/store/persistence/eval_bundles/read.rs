use sqlx::{Postgres, QueryBuilder};

use super::super::{PersistError, PersistHandle};
use super::{BUNDLE_COLUMNS, BundleRow};

#[derive(Debug, Clone, Default)]
pub struct BundleQuery {
    pub role: Option<String>,
    pub subject_key: Option<String>,
    pub origin: Option<String>,
    pub status: Option<String>,
    pub current: Option<bool>,
    pub before_id: Option<i64>,
    pub limit: i64,
}

fn push_bundle_filter(query_builder: &mut QueryBuilder<Postgres>, query: &BundleQuery) {
    query_builder.push(" WHERE TRUE");
    if let Some(role) = query.role.as_deref() {
        query_builder.push(" AND role = ").push_bind(role);
    }
    if let Some(subject_key) = query.subject_key.as_deref() {
        query_builder
            .push(" AND subject_key = ")
            .push_bind(subject_key);
    }
    if let Some(origin) = query.origin.as_deref() {
        query_builder.push(" AND origin = ").push_bind(origin);
    }
    if let Some(status) = query.status.as_deref() {
        query_builder.push(" AND status = ").push_bind(status);
    }
    if query.current == Some(true) {
        query_builder.push(" AND origin = 'resolver' AND superseded_at IS NULL");
    }
    if let Some(before_id) = query.before_id {
        query_builder.push(" AND id < ").push_bind(before_id);
    }
}

impl PersistHandle {
    pub async fn get_bundle(&self, id: i64) -> Result<Option<BundleRow>, PersistError> {
        crate::metrics::time_db("get_bundle", async move {
            let row = sqlx::query_file_as!(
                BundleRow,
                "src/store/persistence/eval_bundles/get_bundle.sql",
                id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn list_bundles(&self, query: &BundleQuery) -> Result<Vec<BundleRow>, PersistError> {
        crate::metrics::time_db("list_bundles", async move {
            let mut builder = QueryBuilder::<Postgres>::new("SELECT ");
            builder.push(BUNDLE_COLUMNS).push(" FROM eval.bundles");
            push_bundle_filter(&mut builder, query);
            builder
                .push(" ORDER BY id DESC LIMIT ")
                .push_bind(query.limit);
            let rows = builder
                .build_query_as::<BundleRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }
}
