SELECT count(*)::bigint
   FROM memory.operator_turns t
  WHERE t.created_at >= now() - interval '9 days'
    AND t.outcome_id = ANY(ARRAY[3, 4]::bigint[])
    AND (t.outcome_id = 3 OR t.outcome_source IS NOT DISTINCT FROM 'frustration_close')
