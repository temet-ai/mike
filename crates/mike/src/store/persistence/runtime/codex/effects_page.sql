SELECT event_id AS "event_id!",
       capture_sequence AS "capture_sequence!",
       effect_ordinal AS "effect_ordinal!",
       observation::text AS "observation!"
  FROM runtime.codex_projection_effects
 WHERE generation_id = $1 AND thread_id = $2
   AND (capture_sequence, effect_ordinal) > ($3::bigint, $4::integer)
 ORDER BY capture_sequence, effect_ordinal
 LIMIT $5
