SELECT count(*)::bigint AS "count!"
   FROM session_judges.verdicts v
   JOIN memory.operator_turns t ON t.id = v.end_turn_id
  WHERE v.mode = 'live' AND v.bundle_id = $1
    AND v.judged_at >= date_trunc('day', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
    AND t.closed_at >= $2 AND t.closed_at < $3
