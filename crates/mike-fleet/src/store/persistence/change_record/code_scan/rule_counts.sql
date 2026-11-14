SELECT f.rule_id AS "rule_id!",
       min(f.rule_source) AS "source!",
       count(*) AS "found!",
       count(*) FILTER (WHERE f.delivered_at IS NOT NULL) AS "delivered!",
       count(*) FILTER (WHERE f.delivered_via = 'push') AS "pushed!",
       count(*) FILTER (WHERE f.outcome = 'fixed') AS "fixed!",
       count(*) FILTER (WHERE f.outcome = 'fixed' AND f.delivered_at IS NOT NULL
                          AND f.outcome_at > f.delivered_at) AS "fixed_after_told!",
       count(*) FILTER (WHERE f.outcome = 'fixed' AND f.delivered_at IS NULL) AS "fixed_untold!",
       count(*) FILTER (WHERE f.outcome = 'fixed_by_other') AS "fixed_by_other!",
       count(*) FILTER (WHERE f.outcome = 'ignored') AS "ignored!",
       count(*) FILTER (WHERE f.outcome = 'open') AS "open!",
       (percentile_cont(0.5) WITHIN GROUP (
            ORDER BY extract(epoch FROM f.delivered_at - f.found_at)))::float8 AS "median_lag_s"
  FROM workspace.code_findings f
 WHERE f.found_at >= $1
 GROUP BY f.rule_id
 ORDER BY count(*) DESC
