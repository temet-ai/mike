SELECT r.name, r.kind, r.action, r.check_name, r.params, r.enabled, r.daily_limit, r.max_resources,
       r.consecutive_refusals, r.paused_at, r.pause_reason, r.measure_after_passes,
       r.ineffective_runs, coalesce(t.actions_today, 0) AS actions_today, r.updated_at
  FROM wright.remedies r
  LEFT JOIN (
      SELECT a.remedy, count(*) AS actions_today
        FROM wright.actions a
       WHERE a.created_at >= date_trunc('day', now())
         AND a.state <> 'refused'
       GROUP BY a.remedy
  ) t ON t.remedy = r.name
 WHERE ($1::text IS NULL OR r.name = $1)
 ORDER BY r.name
