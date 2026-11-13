WITH latest AS (
    SELECT DISTINCT ON (dimension_value, population)
           dimension_value, population, k, n, window_start, window_end
      FROM eval.rates
     WHERE family = 'action_effect' AND dimension = 'guard_question'
     ORDER BY dimension_value, population, window_end DESC, id DESC
)
SELECT qp.question AS "question!", qp.mode AS "mode!",
       hb.k AS "k_holdback?", hb.n AS "n_holdback?",
       hb.window_start AS "window_start_holdback?", hb.window_end AS "window_end_holdback?",
       tr.k AS "k_treated?", tr.n AS "n_treated?",
       tr.window_start AS "window_start_treated?", tr.window_end AS "window_end_treated?"
  FROM steering.question_policies qp
  LEFT JOIN latest hb ON hb.dimension_value = qp.question AND hb.population = 'holdback'
  LEFT JOIN latest tr ON tr.dimension_value = qp.question AND tr.population = 'treated'
 ORDER BY qp.question
