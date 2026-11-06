SELECT r.id AS "id!", r.uuid::text AS "uuid!", rk.code AS "kind!", tr.code AS "trigger!",
       st.code AS "status!", r.started_at AS "started_at!", r.finished_at, r.error,
       COALESCE(ac.action_count, 0) AS "action_count!"
  FROM memory.consolidation_runs r
  JOIN memory.consolidation_run_kinds rk ON rk.id = r.run_kind_id
  JOIN memory.consolidation_triggers tr ON tr.id = r.trigger_id
  JOIN memory.consolidation_run_statuses st ON st.id = r.status_id
  LEFT JOIN (
      SELECT run_id, count(*)::bigint AS action_count
        FROM memory.consolidation_actions
       GROUP BY run_id
  ) ac ON ac.run_id = r.id
 WHERE r.id = $1
