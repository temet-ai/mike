WITH upd AS (
    UPDATE workspace.questions q
       SET state = 'answered', answer = 'an answer', settled_by_session_id = s.id, settled_at = now()
      FROM sessions.sessions s
     WHERE q.question_uuid = '6c95e071-6f3a-4dcf-93e1-9068298e869f'
       AND q.state = 'open'
       AND s.session_uuid = '16f16de3-9652-413d-bb3f-3c370ee2a5a4'
    RETURNING q.*
)
SELECT q.question_uuid AS "question_uuid!", q.run_id,
       a.session_uuid AS "asker!", ag.display_name AS asker_name,
       t.session_uuid AS "to_session?", q.to_task,
       q.about_note_uuid AS about_note, q.question AS "question!", q.blocked AS "blocked!",
       q.state AS "state!", q.answer, sb.session_uuid AS "settled_by?",
       q.created_at AS "created_at!", q.settled_at, q.escalated_at
  FROM upd q
  JOIN sessions.sessions a ON a.id = q.asker_session_id
  LEFT JOIN sessions.session_goals ag ON ag.session_id = a.id
  LEFT JOIN sessions.sessions t ON t.id = q.to_session_id
  LEFT JOIN sessions.sessions sb ON sb.id = q.settled_by_session_id
