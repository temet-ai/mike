INSERT INTO mailbox.messages (message_uuid, to_session_id, from_session_id, body)
SELECT l.brief_note_uuid, s.id, NULL, 'brief ' || l.id || E'\n\n' || l.brief_text
  FROM launch.launches l JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
 WHERE l.session_uuid = 'ad27e68c-8696-4be6-8fea-6646249b18a8' AND l.brief_mode = 'note'
   AND l.brief_note_uuid IS NOT NULL AND l.brief_text IS NOT NULL
   AND NOT EXISTS (SELECT 1 FROM mailbox.messages m WHERE m.message_uuid = l.brief_note_uuid)
 ORDER BY l.id
ON CONFLICT (message_uuid) DO NOTHING
