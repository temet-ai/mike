SELECT EXISTS (
    SELECT 1
      FROM memory.operator_turns ot
      JOIN sessions.sessions s ON s.id = ot.session_id
     WHERE s.session_uuid = $1 AND ot.closed_at >= $2
) AS "holds!"
