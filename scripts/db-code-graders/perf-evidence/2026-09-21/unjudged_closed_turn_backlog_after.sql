SELECT COUNT(*)::bigint, MIN(closed_at)
  FROM memory.operator_turns
 WHERE closed_at   IS NOT NULL
   AND outcome_id  = 1
   AND judge_model IS NULL
   AND judge_failure_reason IS DISTINCT FROM 'permanent'
   AND closed_at   < '2026-09-21 09:53:58+00'::timestamptz
