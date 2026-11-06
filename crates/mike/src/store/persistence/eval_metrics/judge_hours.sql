WITH hours AS (
    SELECT unnest($1::timestamptz[]) AS h),
bounds AS (
    SELECT min(h) - interval '21 days' AS lo, max(h) AS hi FROM hours),
t AS (
    SELECT o.closed_at, o.classified_at, o.judge_model IS NOT NULL AS by_judge
      FROM memory.operator_turns o
      JOIN bounds ON o.closed_at >= bounds.lo AND o.closed_at < bounds.hi
     WHERE o.judge_failure_reason IS DISTINCT FROM 'permanent'
)
SELECT hr.h AS "h!",
       count(t.closed_at) FILTER (WHERE t.closed_at < hr.h - interval '6 hours'
                                    AND (t.classified_at IS NULL OR t.classified_at > hr.h)) AS "stale_6h!",
       count(t.closed_at) FILTER (WHERE t.closed_at < hr.h - interval '30 minutes'
                                    AND (t.classified_at IS NULL OR t.classified_at > hr.h)) AS "waiting_30m!",
       count(t.closed_at) FILTER (WHERE t.by_judge AND t.classified_at >= hr.h - make_interval(hours => $2::int)
                                    AND t.classified_at < hr.h) AS "verdicts!"
  FROM hours hr
  LEFT JOIN t ON t.closed_at < hr.h AND t.closed_at >= hr.h - interval '21 days'
 GROUP BY hr.h
 ORDER BY hr.h
