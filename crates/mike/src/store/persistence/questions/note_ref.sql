SELECT n.note_uuid AS "note_uuid!", s.session_uuid AS "author!", n.title AS "title!",
       n.body AS "body!", n.tags AS "tags!", (n.data ->> 'version')::bigint AS version
  FROM workspace.session_notes n
  JOIN sessions.sessions s ON s.id = n.session_id
 WHERE n.note_uuid = $1
