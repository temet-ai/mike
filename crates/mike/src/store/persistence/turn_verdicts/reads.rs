use sqlx::{Postgres, QueryBuilder};

use super::super::{PersistError, PersistHandle};
use super::{TurnVerdictFilter, TurnVerdictRow, VERDICT_COLUMNS, VERDICT_FROM, VerdictMode};

#[derive(sqlx::FromRow)]
struct CountedRow {
    #[sqlx(flatten)]
    row: TurnVerdictRow,
    total: i64,
}

fn split_total(rows: Vec<CountedRow>) -> (Vec<TurnVerdictRow>, i64) {
    let total = rows.first().map_or(0, |r| r.total);
    (rows.into_iter().map(|r| r.row).collect(), total)
}

fn push_verdict_filter(query: &mut QueryBuilder<Postgres>, f: &TurnVerdictFilter) {
    query.push(" WHERE TRUE");
    if let Some(mode) = f.mode {
        query.push(" AND v.mode = ").push_bind(mode.as_str());
    }
    if let Some(bundle_id) = f.bundle_id {
        query.push(" AND v.bundle_id = ").push_bind(bundle_id);
    }
    if let Some(rubric_version) = f.rubric_version.as_deref() {
        query
            .push(" AND v.rubric_version = ")
            .push_bind(rubric_version);
    }
    if let Some(session_uuid) = f.session_uuid {
        query.push(" AND s.session_uuid = ").push_bind(session_uuid);
    }
    if let Some(since) = f.since {
        query.push(" AND v.created_at >= ").push_bind(since);
    }
    if let Some(until) = f.until {
        query.push(" AND v.created_at < ").push_bind(until);
    }
}

const ALL_MODES: [&str; 3] = ["live", "shadow", "eval"];

impl PersistHandle {
    pub async fn list_turn_verdicts_for_turn(
        &self,
        turn_id: i64,
        mode: Option<VerdictMode>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<TurnVerdictRow>, i64), PersistError> {
        crate::metrics::time_db("list_turn_verdicts_for_turn", async move {
            let modes: Vec<&str> = match mode {
                Some(mode) => vec![mode.as_str()],
                None => ALL_MODES.to_vec(),
            };
            let rows = sqlx::query_file_as!(
                TurnVerdictRow,
                "src/store/persistence/turn_verdicts/verdicts_for_turn.sql",
                turn_id,
                &modes as &[&str],
                limit,
                offset,
            )
            .fetch_all(&self.pool)
            .await?;
            if rows.is_empty() {
                return Ok((rows, 0));
            }
            let total = sqlx::query_scalar!(
                r#"SELECT count(*) AS "total!"
                     FROM memory.turn_verdicts v
                    WHERE v.operator_turn_id = $1 AND v.mode = ANY($2::text[])"#,
                turn_id,
                &modes as &[&str],
            )
            .fetch_one(&self.pool)
            .await?;
            Ok((rows, total))
        })
        .await
    }

    pub async fn list_turn_verdicts(
        &self,
        f: &TurnVerdictFilter,
    ) -> Result<(Vec<TurnVerdictRow>, i64), PersistError> {
        crate::metrics::time_db("list_turn_verdicts", async move {
            let mut query = QueryBuilder::<Postgres>::new("SELECT ");
            query
                .push(VERDICT_COLUMNS)
                .push(", count(*) OVER () AS total ")
                .push(VERDICT_FROM);
            push_verdict_filter(&mut query, f);
            query
                .push(" ORDER BY v.created_at DESC, v.id DESC LIMIT ")
                .push_bind(f.limit)
                .push(" OFFSET ")
                .push_bind(f.offset);
            let rows = query
                .build_query_as::<CountedRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(split_total(rows))
        })
        .await
    }

    pub async fn operator_turn_exists(&self, turn_id: i64) -> Result<bool, PersistError> {
        crate::metrics::time_db("operator_turn_exists", async move {
            let exists = sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM memory.operator_turns WHERE id = $1) AS "exists!""#,
                turn_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(exists)
        })
        .await
    }
}
