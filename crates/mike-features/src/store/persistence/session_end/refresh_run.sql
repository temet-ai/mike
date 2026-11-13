WITH counts AS (
    SELECT count(*) FILTER (WHERE i.outcome = 'judged') AS judged,
           count(*) FILTER (WHERE i.outcome = 'refused') AS refused,
           count(*) FILTER (WHERE i.outcome = 'failed') AS failed,
           coalesce(sum(v.cost_usd), 0) AS cost
      FROM eval.session_end_run_items i
      LEFT JOIN session_judges.verdicts v ON v.id = i.verdict_id AND v.mode = 'eval'
     WHERE i.run_id = $1
)
UPDATE eval.runs r
   SET status = $2,
       items_judged = counts.judged::int,
       items_refused = counts.refused::int,
       items_failed = counts.failed::int,
       cost_usd = counts.cost,
       report = coalesce($3, r.report),
       finished_at = CASE WHEN $2 IN ('complete', 'failed', 'cancelled') THEN now() ELSE r.finished_at END
  FROM counts
 WHERE r.id = $1
