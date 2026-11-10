SELECT c.reference AS "reference!"
  FROM (
    SELECT 'lesson:' || l.uuid AS reference
      FROM memory.lessons l
     WHERE ($2::text IS NULL OR $2 = 'lesson')
       AND l.uuid BETWEEN $3 AND $4
       AND ($1::bigint IS NULL OR l.project_id = $1)
    UNION ALL
    SELECT 'note:' || n.note_uuid
      FROM workspace.session_notes n
      JOIN sessions.sessions s ON s.id = n.session_id
     WHERE ($2::text IS NULL OR $2 = 'note')
       AND n.note_uuid BETWEEN $3 AND $4
       AND ($1::bigint IS NULL OR s.project_id = $1)
    UNION ALL
    SELECT 'thread:' || tx.thread_uuid
      FROM mailbox.thread_text_v tx
     WHERE ($2::text IS NULL OR $2 = 'thread')
       AND tx.thread_uuid BETWEEN $3 AND $4
       AND ($1::bigint IS NULL OR tx.project_id = $1)
    UNION ALL
    SELECT 'question_answer:' || qa.question_uuid
      FROM workspace.questions qa
     WHERE ($2::text IS NULL OR $2 = 'question_answer')
       AND qa.question_uuid BETWEEN $3 AND $4
       AND ($1::bigint IS NULL OR qa.project_id = $1)
    UNION ALL
    SELECT 'mail:' || m.message_uuid
      FROM mailbox.messages m
      LEFT JOIN mailbox.threads th ON th.id = m.thread_id
      LEFT JOIN sessions.sessions fs ON fs.id = m.from_session_id
     WHERE ($2::text IS NULL OR $2 = 'mail')
       AND m.message_uuid BETWEEN $3 AND $4
       AND ($1::bigint IS NULL OR COALESCE(th.project_id, fs.project_id) = $1)
    UNION ALL
    SELECT 'learning:' || k.session_id
      FROM memory.session_knowledge k
      JOIN sessions.sessions s ON s.session_uuid = k.session_id
     WHERE ($2::text IS NULL OR $2 = 'learning')
       AND k.session_id BETWEEN $3 AND $4
       AND ($1::bigint IS NULL OR s.project_id = $1)
  ) c
 ORDER BY c.reference
 LIMIT 10
