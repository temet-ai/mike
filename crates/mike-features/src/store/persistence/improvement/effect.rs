use chrono::{DateTime, NaiveDate, Utc};

use crate::store::persistence::PersistError;
use crate::store::persistence::improvement::interval::wilson_interval;

pub const EFFECT_WINDOW_DAYS: i32 = 14;
pub const ACTION_EFFECT_SQL_VERSION: &str = "action-effect-v2";

pub struct GuardQuestionOutcome {
    pub question: String,
    pub holdback_n: i64,
    pub holdback_bad: i64,
    pub treated_n: i64,
    pub treated_bad: i64,
}

#[derive(Default)]
struct GuardPopulationColumns {
    populations: Vec<String>,
    questions: Vec<String>,
    ks: Vec<i32>,
    ns: Vec<i32>,
    values: Vec<Option<f64>>,
    ci_lows: Vec<Option<f64>>,
    ci_highs: Vec<Option<f64>>,
}

impl GuardPopulationColumns {
    fn push(
        &mut self,
        population: &'static str,
        question: &str,
        k: i64,
        n: i64,
    ) -> Result<(), PersistError> {
        let k32 = i32::try_from(k)
            .map_err(|_| PersistError::DataInvariant(format!("guard outcome k={k} exceeds i32")))?;
        let n32 = i32::try_from(n)
            .map_err(|_| PersistError::DataInvariant(format!("guard outcome n={n} exceeds i32")))?;
        let (value, ci_low, ci_high) = match wilson_interval(k, n) {
            Some(w) => (Some(k as f64 / n as f64), Some(w.low), Some(w.high)),
            None => (None, None, None),
        };
        self.populations.push(population.to_owned());
        self.questions.push(question.to_owned());
        self.ks.push(k32);
        self.ns.push(n32);
        self.values.push(value);
        self.ci_lows.push(ci_low);
        self.ci_highs.push(ci_high);
        Ok(())
    }
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn latest_completed_action_effect_day(
        self,
    ) -> Result<Option<NaiveDate>, PersistError> {
        Ok(sqlx::query_scalar!("SELECT max(for_day) FROM eval.rollups WHERE sql_version = $1 AND finished_at IS NOT NULL", ACTION_EFFECT_SQL_VERSION)
            .fetch_one(&self.pool).await?)
    }

    pub async fn write_action_effect_rows(
        self,
        rollup_id: i64,
        now: DateTime<Utc>,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("write_action_effect_rows", async move {
            let mut tx = self.pool.begin().await?;
            sqlx::query!("SELECT id FROM eval.rollups WHERE id = $1 FOR UPDATE", rollup_id)
                .fetch_one(&mut *tx).await?;
            sqlx::query!("DELETE FROM eval.rates WHERE rollup_id = $1 AND family = 'action_effect' AND dimension = 'lesson'", rollup_id)
                .execute(&mut *tx).await?;
            let lesson_rows = sqlx::query!(
                "WITH promotions AS (
    SELECT (cl.subject_key)::bigint AS lesson_id, min(cl.changed_at) AS promoted_at
      FROM config.change_log cl
     WHERE cl.subject_kind = 'lesson_decision' AND cl.new_value ->> 'stage' = 'daily'
     GROUP BY cl.subject_key
),
targets AS (
    SELECT l.id AS lesson_id, l.project_id, lsrc.code AS source_code,
           COALESCE(pr.promoted_at, l.created_at) AS anchor_at
      FROM memory.lessons l
      JOIN memory.lesson_sources lsrc ON lsrc.id = l.source_id
      JOIN memory.lesson_stages ls ON ls.id = l.stage_id
      LEFT JOIN promotions pr ON pr.lesson_id = l.id
     WHERE lsrc.code IN ('judge', 'directive', 'operator')
       AND (ls.code IN ('daily', 'long_term') OR (ls.code = 'raw' AND lsrc.code = 'operator'))
       AND l.project_id IS NOT NULL
),
ranked AS (
    SELECT lb.operator_turn_id, lb.label, lb.tier, lb.created_at,
           row_number() OVER (PARTITION BY lb.operator_turn_id
                              ORDER BY CASE lb.tier WHEN 'gold' THEN 1 WHEN 'relayed' THEN 2
                                                   WHEN 'seed' THEN 3 ELSE 4 END,
                                       lb.created_at DESC, lb.id DESC) AS rk
      FROM eval.labels_as_of($2) lb
     WHERE lb.split = 'dev' AND lb.subject_kind = 'turn' AND lb.axis = 'reaction'
),
best AS (SELECT operator_turn_id, label FROM ranked WHERE rk = 1),
scored AS (
    SELECT t.lesson_id, t.project_id, t.anchor_at, p.created_at,
           (b.label = 'corrected') AS is_corrected
      FROM targets t
      JOIN eval.turn_population_v p ON p.project_id = t.project_id
      JOIN best b ON b.operator_turn_id = p.operator_turn_id
     WHERE p.is_real AND p.is_replied
       AND p.created_at >= t.anchor_at - make_interval(days => $3)
       AND p.created_at < t.anchor_at + make_interval(days => $3)
),
buckets AS (
    SELECT t.lesson_id, t.project_id, t.anchor_at, pop.population,
           count(s.lesson_id) FILTER (WHERE s.is_corrected) AS k, count(s.lesson_id) AS n
      FROM targets t CROSS JOIN (VALUES ('before'), ('after')) pop(population)
      LEFT JOIN scored s ON s.lesson_id = t.lesson_id
         AND (CASE WHEN s.created_at < t.anchor_at THEN 'before' ELSE 'after' END) = pop.population
     GROUP BY t.lesson_id, t.project_id, t.anchor_at, pop.population
)
INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, project_id,
                        window_start, window_end, window_days, role, axis, class,
                        dimension, dimension_value, k, n, value, ci_low, ci_high,
                        weighted, detail)
SELECT $1, 'action_effect', 'share', b.population, 'project', b.project_id,
       CASE WHEN b.population = 'before' THEN b.anchor_at - make_interval(days => $3)
            ELSE b.anchor_at END,
       CASE WHEN b.population = 'before' THEN b.anchor_at
            ELSE b.anchor_at + make_interval(days => $3) END,
       $3, 'turn_judge', 'reaction', 'corrected', 'lesson', b.lesson_id::text,
       b.k::int, b.n::int, b.k::float8 / nullif(b.n, 0),
       eval.wilson_low(b.k::float8 / nullif(b.n, 0), b.n),
       eval.wilson_high(b.k::float8 / nullif(b.n, 0), b.n),
       false, jsonb_build_object('anchor_at', b.anchor_at, 'measure', 'observational_project_before_after')
  FROM buckets b",
                rollup_id,
                now,
                EFFECT_WINDOW_DAYS
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            let row_count = i32::try_from(lesson_rows).map_err(|_| PersistError::DataInvariant("effect row count exceeds i32".into()))?;
            sqlx::query!("UPDATE eval.rollups SET finished_at = NOW(), rows_written = $2 WHERE id = $1", rollup_id, row_count)
                .execute(&mut *tx).await?;
            tx.commit().await?;
            Ok(lesson_rows)
        })
        .await
    }

    pub async fn write_guard_effect_rows(
        self,
        rollup_id: i64,
        now: DateTime<Utc>,
        outcomes: &[GuardQuestionOutcome],
    ) -> Result<u64, PersistError> {
        if outcomes.is_empty() {
            return Ok(0);
        }
        let mut columns = GuardPopulationColumns::default();
        for outcome in outcomes {
            columns.push(
                "holdback",
                &outcome.question,
                outcome.holdback_bad,
                outcome.holdback_n,
            )?;
            columns.push(
                "treated",
                &outcome.question,
                outcome.treated_bad,
                outcome.treated_n,
            )?;
        }
        crate::metrics::time_db("write_guard_effect_rows", async move {
            Ok(sqlx::query_file!(
                "src/store/persistence/improvement/insert_guard_effect_rows.sql",
                rollup_id,
                now,
                EFFECT_WINDOW_DAYS,
                &columns.populations,
                &columns.questions,
                &columns.ks,
                &columns.ns,
                &columns.values as &[Option<f64>],
                &columns.ci_lows as &[Option<f64>],
                &columns.ci_highs as &[Option<f64>],
            )
            .execute(&self.pool)
            .await?
            .rows_affected())
        })
        .await
    }

    pub async fn current_or_claimed_action_effect_rollup_id(
        self,
        for_day: NaiveDate,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("current_or_claimed_action_effect_rollup_id", async move {
            Ok(sqlx::query_scalar!(
                r#"INSERT INTO eval.rollups (for_day, sql_version, mode, labels_as_of)
                   VALUES ($1, $2, 'manual', NOW())
                   ON CONFLICT (for_day, sql_version) DO UPDATE SET mode = eval.rollups.mode
                   RETURNING id AS "id!""#,
                for_day,
                ACTION_EFFECT_SQL_VERSION,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }
}
