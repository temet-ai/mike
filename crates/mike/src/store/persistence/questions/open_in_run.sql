SELECT q.question_uuid AS "question_uuid!", q.run_id,
       a.session_uuid AS "asker!", ag.display_name AS asker_name,
       t.session_uuid AS "to_session?", q.to_task,
       q.about_note_uuid AS about_note, q.question AS "question!", q.blocked AS "blocked!",
       q.state AS "state!", q.answer, sb.session_uuid AS "settled_by?",
       q.created_at AS "created_at!", q.settled_at, q.escalated_at
  FROM workspace.questions q
  JOIN sessions.sessions a ON a.id = q.asker_session_id
  LEFT JOIN sessions.session_goals ag ON ag.session_id = a.id
  LEFT JOIN sessions.sessions t ON t.id = q.to_session_id
  LEFT JOIN sessions.sessions sb ON sb.id = q.settled_by_session_id
 WHERE q.run_id = $1 AND q.state = 'open'
 ORDER BY q.created_at, q.id
