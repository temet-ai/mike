SELECT ot.id AS operator_turn_id, s.session_uuid, s.title AS session_title,
       ot.closed_at AS turn_closed_at
  FROM memory.operator_turns ot
  JOIN sessions.sessions s ON s.id = ot.session_id
 WHERE ot.id = ANY(ARRAY[18442,18441,18440,18439,18438]::bigint[]);
