WITH last_turn AS (
    SELECT DISTINCT ON (ot.session_id)
           ot.session_id, ot.closed_at
      FROM memory.operator_turns ot
     WHERE ot.closed_at > now() - make_interval(hours => $1::int)
       AND ot.closed_at < now() - make_interval(secs => $2::int)
     ORDER BY ot.session_id, ot.closed_at DESC
)
SELECT s.id AS "session_id!",
       s.session_uuid AS "session_uuid!",
       s.title AS "title?",
       lt.closed_at AS "last_closed_at!",
       sg.goal AS "goal?",
       sg.next_steps AS "next_steps?",
       sg.display_name AS "display_name?",
       sg.goal_source AS "goal_source?"
  FROM last_turn lt
  JOIN sessions.sessions s ON s.id = lt.session_id
  LEFT JOIN sessions.session_goals sg ON sg.session_id = s.id
 WHERE s.flow_producer IS NOT NULL
   AND (sg.session_id IS NULL
        OR (sg.updated_at < lt.closed_at
            AND sg.updated_at < now() - make_interval(mins => $3::int)))
 ORDER BY lt.closed_at DESC
 LIMIT $4
