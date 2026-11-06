SELECT lv.reaction AS live_reaction, sv.reaction AS shadow_reaction,
       count(*)::bigint AS n
  FROM memory.turn_verdicts lv
  JOIN memory.turn_verdicts sv ON sv.operator_turn_id = lv.operator_turn_id
 WHERE lv.mode = 'live' AND lv.bundle_id = 14
   AND sv.mode = 'shadow' AND sv.bundle_id = 14
   AND lv.reaction IS NOT NULL AND sv.reaction IS NOT NULL
   AND lv.created_at >= now()::timestamptz - interval '14 days' AND lv.created_at < now()::timestamptz
 GROUP BY lv.reaction, sv.reaction