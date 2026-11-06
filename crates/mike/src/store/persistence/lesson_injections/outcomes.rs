use super::super::{PersistError, PersistHandle};

pub struct LessonOutcomeRow {
    pub lesson_id: i64,
    pub lesson_text: String,
    pub injection_count: i64,
    pub distinct_session_count: i64,
    pub outcome: Option<String>,
    pub outcome_count: Option<i64>,
    pub mean_quality_score: Option<f64>,
}

impl PersistHandle {
    pub async fn lesson_outcomes_observed_after_injection(
        &self,
    ) -> Result<Vec<LessonOutcomeRow>, PersistError> {
        crate::metrics::time_db("lesson_outcomes_observed_after_injection", async move {
            let rows = sqlx::query_as!(
                LessonOutcomeRow,
                r#"WITH per_lesson_injections AS (
                     SELECT lesson_id,
                            count(*)                    AS injection_count,
                            count(DISTINCT session_id)   AS distinct_session_count
                       FROM memory.lesson_injections
                      GROUP BY lesson_id
                 ),
                 per_lesson_verdicts AS (
                     SELECT DISTINCT li.lesson_id, v.id AS verdict_id, v.outcome_id, v.quality_score
                       FROM memory.lesson_injections li
                       JOIN session_judges.verdicts v
                         ON v.session_id = li.session_id
                        AND v.judged_at  > li.injected_at
                        AND v.mode = 'live'
                 ),
                 per_lesson_outcome_counts AS (
                     SELECT pv.lesson_id, o.code AS outcome, count(*) AS outcome_count
                       FROM per_lesson_verdicts pv
                       JOIN session_judges.outcomes o ON o.id = pv.outcome_id
                      GROUP BY pv.lesson_id, o.code
                 ),
                 per_lesson_quality AS (
                     SELECT lesson_id, avg(quality_score)::float8 AS mean_quality_score
                       FROM per_lesson_verdicts
                      GROUP BY lesson_id
                 )
                 SELECT l.id                          AS "lesson_id!",
                        l.text                         AS "lesson_text!",
                        pli.injection_count            AS "injection_count!",
                        pli.distinct_session_count     AS "distinct_session_count!",
                        ploc.outcome                   AS "outcome?",
                        ploc.outcome_count             AS "outcome_count?",
                        plq.mean_quality_score         AS "mean_quality_score?"
                   FROM memory.lessons l
                   JOIN per_lesson_injections pli ON pli.lesson_id = l.id
              LEFT JOIN per_lesson_outcome_counts ploc ON ploc.lesson_id = l.id
              LEFT JOIN per_lesson_quality        plq  ON plq.lesson_id = l.id
                  ORDER BY l.id, ploc.outcome"#,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
