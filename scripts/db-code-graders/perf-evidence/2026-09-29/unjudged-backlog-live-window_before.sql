SELECT COUNT(*)::bigint, MIN(closed_at)
  FROM memory.operator_turns
 WHERE closed_at   IS NOT NULL
   AND outcome_id  = 1
   AND judge_model IS NULL
   AND judge_failure_reason IS DISTINCT FROM 'permanent'
   AND (opened_by IS NULL OR opened_by = 'operator')
   AND closed_at   < now() - interval '6 hours';
