WITH ins AS (
    INSERT INTO workspace.questions
        (project_id, run_id, asker_session_id, to_session_id, to_task, about_note_uuid, question, blocked)
    SELECT a.project_id, $2, a.id, t.id, $4, $5, $6, $7
      FROM sessions.sessions a
      LEFT JOIN sessions.sessions t ON t.session_uuid = $3
     WHERE a.session_uuid = $1
       AND ($3::uuid IS NULL OR t.id IS NOT NULL)
    RETURNING question_uuid, run_id, asker_session_id, to_session_id, to_task,
              about_note_uuid, question, blocked, state, answer,
              settled_by_session_id, created_at, settled_at, escalated_at
)
SELECT q.question_uuid AS "question_uuid!", q.run_id,
       a.session_uuid AS "asker!", ag.display_name AS asker_name,
       t.session_uuid AS "to_session?", q.to_task,
       q.about_note_uuid AS about_note, q.question AS "question!", q.blocked AS "blocked!",
       q.state AS "state!", q.answer, sb.session_uuid AS "settled_by?",
       q.created_at AS "created_at!", q.settled_at, q.escalated_at
  FROM ins q
  JOIN sessions.sessions a ON a.id = q.asker_session_id
  LEFT JOIN sessions.session_goals ag ON ag.session_id = a.id
  LEFT JOIN sessions.sessions t ON t.id = q.to_session_id
  LEFT JOIN sessions.sessions sb ON sb.id = q.settled_by_session_id
