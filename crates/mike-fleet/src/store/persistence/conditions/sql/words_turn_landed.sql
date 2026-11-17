SELECT s.id AS "session_id!",
       EXISTS (
           SELECT 1
             FROM memory.operator_turns ot
            WHERE ot.session_id = s.id
              AND ot.closed_at >= GREATEST(c.not_before, $3::timestamptz,COALESCE(c.judged_through, c.not_before))
       ) AS "landed!"
  FROM launch.conditions c
  JOIN sessions.sessions s ON s.session_uuid = $2
 WHERE c.id = $1
