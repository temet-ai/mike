WITH ins AS (
    INSERT INTO workspace.questions
        (project_id, run_id, asker_session_id, to_session_id, to_task, about_note_uuid, question, blocked)
    SELECT a.project_id, 'run-141', a.id, t.id, 'T3', NULL::uuid, 'a question', true
      FROM sessions.sessions a
      LEFT JOIN sessions.sessions t ON t.session_uuid = '57ff6ad6-10e9-45af-82f4-8b9cbe7350ac'
     WHERE a.session_uuid = '57ff6ad6-10e9-45af-82f4-8b9cbe7350ac'
       AND ('57ff6ad6-10e9-45af-82f4-8b9cbe7350ac'::uuid IS NULL OR t.id IS NOT NULL)
    RETURNING *
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
