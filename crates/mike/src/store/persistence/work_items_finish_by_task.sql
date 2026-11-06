UPDATE agents.work_items
   SET state = $1,
       finished_at = $2,
       finished_request_id = $3,
       notified_agent_id = COALESCE($4, notified_agent_id),
       exit_code = $5,
       summary = COALESCE($6, summary)
 WHERE session_id = $7
   AND (finished_at IS NULL OR finished_at <= $2)
   AND owner_agent_id IS NOT DISTINCT FROM $4
   AND task_id = $8
