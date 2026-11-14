SELECT f.rule_id AS "rule_id!",
       count(*) AS "found!",
       count(*) FILTER (WHERE f.delivered_request_id IS NOT NULL) AS "delivered!",
       count(*) FILTER (WHERE f.outcome = 'fixed') AS "fixed!",
       count(*) FILTER (WHERE f.outcome = 'fixed' AND f.delivered_request_id IS NOT NULL
                          AND f.outcome_at > f.delivered_at) AS "fixed_after_told!",
       count(*) FILTER (WHERE f.outcome = 'open') AS "open!"
  FROM workspace.code_findings f
 WHERE f.found_at >= $1
 GROUP BY f.rule_id
 ORDER BY count(*) DESC
