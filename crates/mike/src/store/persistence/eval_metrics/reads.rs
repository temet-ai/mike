use sqlx::{Postgres, QueryBuilder};

use super::super::{PersistError, PersistHandle};
use super::types::{
    AckOutcome, AlarmState, AlarmsFilter, EvalAlarmRow, EvalRateRow, EvalRollupRow, RatesFilter,
};

const RATES_SELECT: &str = "SELECT r.id, r.rollup_id, ro.for_day, r.family, r.metric, r.population,
        r.scope_kind, r.project_id, r.window_start, r.window_end, r.window_days, r.role,
        r.bundle_id, r.bundle_label, r.axis, r.class, r.label_tier, r.label_split, r.label_source,
        r.dimension, r.dimension_value, r.steerer_id, r.question, r.mode, r.k, r.n, r.value,
        r.ci_low, r.ci_high, r.weighted, r.n_effective, r.detail, count(*) OVER () AS total
   FROM eval.rates r
   JOIN eval.rollups ro ON ro.id = r.rollup_id AND ro.finished_at IS NOT NULL";

const RATES_ORDER: &str =
    " ORDER BY ro.for_day DESC, r.family, r.role, r.axis, r.class, r.label_tier,
          r.label_split, r.label_source, r.bundle_label NULLS FIRST, r.window_days NULLS FIRST,
          r.dimension_value, r.id LIMIT ";

const NEWEST_ROLLUP: &str = "(SELECT id FROM eval.rollups WHERE finished_at IS NOT NULL
        ORDER BY for_day DESC, started_at DESC, id DESC LIMIT 1)";

fn rates_query(f: &RatesFilter) -> QueryBuilder<Postgres> {
    let mut qb = QueryBuilder::new(RATES_SELECT);
    if f.project.is_some() {
        qb.push(" JOIN projects.projects pr ON pr.id = r.project_id");
    }
    qb.push(" WHERE true");
    if f.latest {
        qb.push(" AND r.rollup_id = ").push(NEWEST_ROLLUP);
    }
    let text_filters = [
        ("r.family", &f.family),
        ("r.metric", &f.metric),
        ("r.population", &f.population),
        ("r.role", &f.role),
        ("r.bundle_label", &f.bundle_label),
        ("r.axis", &f.axis),
        ("r.class", &f.class),
        ("r.label_tier", &f.label_tier),
        ("r.label_split", &f.label_split),
        ("r.label_source", &f.label_source),
        ("r.scope_kind", &f.scope),
        ("r.question", &f.question),
        ("r.dimension", &f.dimension),
    ];
    for (column, value) in text_filters {
        if let Some(value) = value {
            qb.push(" AND ").push(column).push(" = ").push_bind(value);
        }
    }
    if let Some(bundle_id) = f.bundle_id {
        qb.push(" AND r.bundle_id = ").push_bind(bundle_id);
    }
    if let Some(project) = &f.project {
        qb.push(" AND pr.project_path ILIKE '%' || ")
            .push_bind(project)
            .push(" || '%'");
    }
    match f.window_days {
        Some(0) => {
            qb.push(" AND (r.window_days IS NULL OR r.window_days = 0)");
        }
        Some(days) => {
            qb.push(" AND r.window_days = ").push_bind(days);
        }
        None => {}
    }
    if let Some(steerer_id) = f.steerer_id {
        qb.push(" AND r.steerer_id = ").push_bind(steerer_id);
    }
    if let Some(rollup_id) = f.rollup_id {
        qb.push(" AND r.rollup_id = ").push_bind(rollup_id);
    }
    if let Some(from_day) = f.from_day {
        qb.push(" AND ro.for_day >= ").push_bind(from_day);
    }
    if let Some(to_day) = f.to_day {
        qb.push(" AND ro.for_day <= ").push_bind(to_day);
    }
    qb.push(RATES_ORDER)
        .push_bind(f.limit)
        .push(" OFFSET ")
        .push_bind(f.offset);
    qb
}

const ALARMS_SELECT: &str = "SELECT a.id, a.kind, a.subject, a.first_seen_at, a.last_seen_at,
        a.cleared_at, a.evaluations, a.quiet_evaluations, a.detail, a.acknowledged_at, a.ack_note,
        count(*) OVER () AS total
   FROM eval.alarms a";

fn alarms_query(f: &AlarmsFilter) -> QueryBuilder<Postgres> {
    let mut qb = QueryBuilder::new(ALARMS_SELECT);
    qb.push(" WHERE true");
    match f.state {
        AlarmState::Open => {
            qb.push(" AND a.cleared_at IS NULL");
        }
        AlarmState::Cleared => {
            qb.push(" AND a.cleared_at IS NOT NULL");
        }
        AlarmState::All => {}
    }
    if let Some(kind) = &f.kind {
        qb.push(" AND a.kind = ").push_bind(kind);
    }
    match f.acknowledged {
        Some(true) => {
            qb.push(" AND a.acknowledged_at IS NOT NULL");
        }
        Some(false) => {
            qb.push(" AND a.acknowledged_at IS NULL");
        }
        None => {}
    }
    if let Some(since) = f.since {
        qb.push(" AND a.first_seen_at >= ").push_bind(since);
    }
    qb.push(" ORDER BY a.cleared_at IS NULL DESC, a.first_seen_at DESC, a.id DESC LIMIT ")
        .push_bind(f.limit)
        .push(" OFFSET ")
        .push_bind(f.offset);
    qb
}

#[derive(sqlx::FromRow)]
struct CountedRate {
    #[sqlx(flatten)]
    row: EvalRateRow,
    total: i64,
}

#[derive(sqlx::FromRow)]
struct CountedAlarm {
    #[sqlx(flatten)]
    row: EvalAlarmRow,
    total: i64,
}

impl PersistHandle {
    pub async fn newest_eval_rollup(&self) -> Result<Option<EvalRollupRow>, PersistError> {
        crate::metrics::time_db("newest_eval_rollup", async move {
            Ok(sqlx::query_as!(
                EvalRollupRow,
                "SELECT id, for_day, sql_version, mode, labels_as_of, started_at, finished_at,
                        rows_written
                   FROM eval.rollups WHERE finished_at IS NOT NULL
                  ORDER BY for_day DESC, started_at DESC, id DESC LIMIT 1",
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn list_eval_rates(
        &self,
        f: &RatesFilter,
    ) -> Result<(Vec<EvalRateRow>, i64), PersistError> {
        crate::metrics::time_db("list_eval_rates", async move {
            let rows = rates_query(f)
                .build_query_as::<CountedRate>()
                .fetch_all(&self.pool)
                .await?;
            let total = rows.first().map_or(0, |r| r.total);
            Ok((rows.into_iter().map(|r| r.row).collect(), total))
        })
        .await
    }

    pub async fn list_eval_alarms(
        &self,
        f: &AlarmsFilter,
    ) -> Result<(Vec<EvalAlarmRow>, i64), PersistError> {
        crate::metrics::time_db("list_eval_alarms", async move {
            let rows = alarms_query(f)
                .build_query_as::<CountedAlarm>()
                .fetch_all(&self.pool)
                .await?;
            let total = rows.first().map_or(0, |r| r.total);
            Ok((rows.into_iter().map(|r| r.row).collect(), total))
        })
        .await
    }

    pub async fn acknowledge_eval_alarm(
        &self,
        id: i64,
        note: &str,
        replace_note: bool,
    ) -> Result<(AckOutcome, Option<EvalAlarmRow>), PersistError> {
        crate::metrics::time_db("acknowledge_eval_alarm", async move {
            let updated = sqlx::query_as!(
                EvalAlarmRow,
                r#"UPDATE eval.alarms
                      SET acknowledged_at = now(), ack_note = $2
                    WHERE id = $1 AND (acknowledged_at IS NULL OR $3)
                    RETURNING id AS "id!", kind AS "kind!", subject AS "subject!",
                              first_seen_at AS "first_seen_at!", last_seen_at AS "last_seen_at!",
                              cleared_at AS "cleared_at?", evaluations AS "evaluations!",
                              quiet_evaluations AS "quiet_evaluations!", detail AS "detail!",
                              acknowledged_at AS "acknowledged_at?", ack_note AS "ack_note?""#,
                id,
                note,
                replace_note,
            )
            .fetch_optional(&self.pool)
            .await?;
            if let Some(row) = updated {
                return Ok((AckOutcome::Acknowledged, Some(row)));
            }
            let existing = sqlx::query_as!(
                EvalAlarmRow,
                "SELECT id, kind, subject, first_seen_at, last_seen_at, cleared_at, evaluations,
                        quiet_evaluations, detail, acknowledged_at, ack_note
                   FROM eval.alarms WHERE id = $1",
                id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(match existing {
                Some(row) => (AckOutcome::AlreadyAcknowledged, Some(row)),
                None => (AckOutcome::NotFound, None),
            })
        })
        .await
    }
}
