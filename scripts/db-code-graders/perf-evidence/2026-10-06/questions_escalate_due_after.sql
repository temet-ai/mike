WITH due AS (
    UPDATE workspace.questions q
       SET escalated_at = now()
     WHERE q.run_id = 'run-141'
       AND q.state = 'open'
       AND q.blocked
       AND q.escalated_at IS NULL
       AND q.created_at <= now() - make_interval(mins => 5)
    RETURNING q.*
)
SELECT q.question_uuid AS "question_uuid!", q.run_id,
       a.session_uuid AS "asker!", ag.display_name AS asker_name,
       t.session_uuid AS "to_session?", q.to_task,
       q.about_note_uuid AS about_note, q.question AS "question!", q.blocked AS "blocked!",
       q.state AS "state!", q.answer, sb.session_uuid AS "settled_by?",
       q.created_at AS "created_at!", q.settled_at, q.escalated_at
  FROM due q
  JOIN sessions.sessions a ON a.id = q.asker_session_id
  LEFT JOIN sessions.session_goals ag ON ag.session_id = a.id
  LEFT JOIN sessions.sessions t ON t.id = q.to_session_id
  LEFT JOIN sessions.sessions sb ON sb.id = q.settled_by_session_id
 ORDER BY q.created_at, q.id
