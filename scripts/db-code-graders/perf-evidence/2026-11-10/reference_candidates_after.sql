SELECT c.reference
  FROM (
    SELECT 'lesson:' || l.uuid AS reference
      FROM memory.lessons l
     WHERE (NULL::text IS NULL OR NULL::text = 'lesson')
       AND l.uuid BETWEEN 'e9fb06f6-0000-0000-0000-000000000000'::uuid AND 'e9fb06f6-ffff-ffff-ffff-ffffffffffff'::uuid
       AND (993478::bigint IS NULL OR l.project_id = 993478)
    UNION ALL
    SELECT 'note:' || n.note_uuid
      FROM workspace.session_notes n
      JOIN sessions.sessions s ON s.id = n.session_id
     WHERE (NULL::text IS NULL OR NULL::text = 'note')
       AND n.note_uuid BETWEEN 'e9fb06f6-0000-0000-0000-000000000000'::uuid AND 'e9fb06f6-ffff-ffff-ffff-ffffffffffff'::uuid
       AND (993478::bigint IS NULL OR s.project_id = 993478)
    UNION ALL
    SELECT 'thread:' || tx.thread_uuid
      FROM mailbox.thread_text_v tx
     WHERE (NULL::text IS NULL OR NULL::text = 'thread')
       AND tx.thread_uuid BETWEEN 'e9fb06f6-0000-0000-0000-000000000000'::uuid AND 'e9fb06f6-ffff-ffff-ffff-ffffffffffff'::uuid
       AND (993478::bigint IS NULL OR tx.project_id = 993478)
    UNION ALL
    SELECT 'question_answer:' || qa.question_uuid
      FROM workspace.questions qa
     WHERE (NULL::text IS NULL OR NULL::text = 'question_answer')
       AND qa.question_uuid BETWEEN 'e9fb06f6-0000-0000-0000-000000000000'::uuid AND 'e9fb06f6-ffff-ffff-ffff-ffffffffffff'::uuid
       AND (993478::bigint IS NULL OR qa.project_id = 993478)
    UNION ALL
    SELECT 'mail:' || m.message_uuid
      FROM mailbox.messages m
      LEFT JOIN mailbox.threads th ON th.id = m.thread_id
      LEFT JOIN sessions.sessions fs ON fs.id = m.from_session_id
     WHERE (NULL::text IS NULL OR NULL::text = 'mail')
       AND m.message_uuid BETWEEN 'e9fb06f6-0000-0000-0000-000000000000'::uuid AND 'e9fb06f6-ffff-ffff-ffff-ffffffffffff'::uuid
       AND (993478::bigint IS NULL OR COALESCE(th.project_id, fs.project_id) = 993478)
    UNION ALL
    SELECT 'learning:' || k.session_id
      FROM memory.session_knowledge k
      JOIN sessions.sessions s ON s.session_uuid = k.session_id
     WHERE (NULL::text IS NULL OR NULL::text = 'learning')
       AND k.session_id BETWEEN 'e9fb06f6-0000-0000-0000-000000000000'::uuid AND 'e9fb06f6-ffff-ffff-ffff-ffffffffffff'::uuid
       AND (993478::bigint IS NULL OR s.project_id = 993478)
  ) c
 ORDER BY c.reference
 LIMIT 10
