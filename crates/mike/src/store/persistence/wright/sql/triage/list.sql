SELECT q.cause_id, q.problem_id, q.check_name, q.resource, q.problems, q.observations,
       q.opened_at, q.attempts, q.last_attempt_at, q.finding_id, q.followup_id, q.issue_number,
       q.remedied, q.priority, c.notified_at, c.commented_at,
       q.pinned, c.pinned_at, q.operator_priority, q.rank_priority, q.group_id,
       g.name AS group_name,
       ab.absorbed,
       (w.why IS NULL) AS investigable,
       w.why AS not_investigated_because
  FROM wright.cause_queue q
  JOIN wright.causes c ON c.id = q.cause_id
  LEFT JOIN wright.cause_groups g ON g.id = q.group_id
  CROSS JOIN wright.followup_limits fl
  CROSS JOIN LATERAL (
      SELECT CASE
                 WHEN q.finding_id IS NOT NULL THEN 'finding'
                 WHEN q.followup_id IS NOT NULL THEN 'followup'
                 WHEN q.remedied THEN 'known_fix'
                 WHEN q.attempts >= fl.max_attempts THEN 'attempts'
             END AS why
  ) w
  CROSS JOIN LATERAL (
      SELECT COALESCE(
                 jsonb_agg(jsonb_build_object(
                     'merge_id', m.id,
                     'cause_id', m.absorbed_cause_id,
                     'check_name', a.check_name,
                     'key', a.key,
                     'problems', cardinality(m.problem_ids),
                     'by', m.by,
                     'created_at', m.created_at,
                     'can_separate', m.by = 'operator') ORDER BY m.id DESC),
                 '[]'::jsonb) AS absorbed
        FROM wright.cause_merges m
        JOIN wright.causes a ON a.id = m.absorbed_cause_id
       WHERE m.root_cause_id = q.cause_id
         AND m.separated_at IS NULL
  ) ab
 WHERE fl.id
   AND ($2::bigint IS NULL OR q.cause_id = $2)
 ORDER BY q.pinned DESC, q.rank_priority DESC, q.cause_id
 LIMIT $1
