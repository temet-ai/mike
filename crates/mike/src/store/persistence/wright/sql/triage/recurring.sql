SELECT q.cause_id, q.followup_id, q.issue_number, q.check_name, q.resource,
       q.problems, q.observations,
       COALESCE(c.commented_at, fu.filed_at, fu.created_at) AS since
  FROM wright.cause_queue q
  JOIN wright.causes c ON c.id = q.cause_id
  JOIN wright.followups fu ON fu.id = q.followup_id
 WHERE q.issue_number IS NOT NULL
   AND (c.commented_at IS NULL OR c.commented_at < now() - interval '24 hours')
   AND EXISTS (
       SELECT 1
         FROM wright.problems p
        WHERE p.cause_id = q.cause_id
          AND p.last_seen_at > COALESCE(c.commented_at, fu.filed_at, fu.created_at))
 ORDER BY q.pinned DESC, q.rank_priority DESC, q.cause_id
 LIMIT $1
