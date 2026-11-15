SELECT r.point AS "point!",
       count(*) AS "calls!",
       count(*) FILTER (WHERE r.mode = 'on' AND r.applied IS NOT NULL AND r.applied <> 'none') AS "applied_on!",
       count(*) FILTER (WHERE r.fail_open) AS "fail_open!",
       count(*) FILTER (WHERE r.outcome = 'schema_error') AS "schema_errors!",
       count(*) FILTER (WHERE r.outcome LIKE 'refused_%' OR r.outcome = 'no_key') AS "refused!",
       COALESCE(sum(r.question_refusals), 0)::bigint AS "question_refusals!",
       COALESCE(sum(r.cost_usd), 0)::float8 AS "cost_usd!",
       percentile_cont(0.5) WITHIN GROUP (ORDER BY r.latency_ms)::float8 AS "latency_p50_ms?",
       percentile_cont(0.95) WITHIN GROUP (ORDER BY r.latency_ms)::float8 AS "latency_p95_ms?",
       count(o.record_id) AS "labelled!",
       count(o.record_id) FILTER (WHERE o.first_label >= now() - interval '7 days') AS "labelled_last_7d!"
  FROM decisions.records r
  LEFT JOIN (SELECT x.record_id, min(x.labelled_at) AS first_label
               FROM decisions.outcomes x
               JOIN decisions.records rr ON rr.id = x.record_id
               LEFT JOIN unnest($2::text[], $3::text[]) AS retired(point, label_source)
                 ON retired.point = rr.point AND retired.label_source = x.label_source
              WHERE rr.created_at >= $1 AND rr.mode IN ('on', 'shadow')
                AND retired.point IS NULL
              GROUP BY x.record_id) o ON o.record_id = r.id
 WHERE r.created_at >= $1 AND r.mode IN ('on', 'shadow')
 GROUP BY r.point
 ORDER BY r.point
