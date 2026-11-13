INSERT INTO memory.lessons(uuid, project_id, text, text_sha256, kind_id, stage_id,
    source_id, topic, expires_at, archived_at, archived_reason_id, agent_id,
    session_id, created_at, portable_origin, duplicate_of_id)
SELECT $2, $1, $3->>'body', $4, k.id, st.id, src.id, $3->>'topic',
    ($3->>'expires_at')::timestamptz, ($3->>'archived_at')::timestamptz,
    ar.id, a.id, s.id, COALESCE(($3->>'created_at')::timestamptz, now()),
    jsonb_build_object('session_uuid', $3->'session_uuid', 'source_turn_id', $3->'source_turn_id',
      'duplicate_of_uuid', $3->'duplicate_of_uuid'), dup.id
FROM memory.lesson_kinds k
JOIN memory.lesson_stages st ON st.code = ($3::jsonb)->>'stage'
JOIN memory.lesson_sources src ON src.code = $3->>'source'
LEFT JOIN memory.lesson_archived_reasons ar ON ar.code = $3->>'archived_reason'
LEFT JOIN agents.agents a ON a.uuid = ($3->>'agent_uuid')::uuid
LEFT JOIN sessions.sessions s ON s.session_uuid = ($3->>'session_uuid')::uuid AND s.project_id = $1
LEFT JOIN memory.lessons dup ON dup.uuid = ($3->>'duplicate_of_uuid')::uuid AND dup.project_id = $1
WHERE ($3->>'duplicate_of_uuid' IS NULL OR dup.id IS NOT NULL) AND k.code = $3->>'kind' AND ($3->>'agent_uuid' IS NULL OR a.id IS NOT NULL)
ON CONFLICT(uuid) DO UPDATE SET text = EXCLUDED.text, text_sha256 = EXCLUDED.text_sha256,
    kind_id = EXCLUDED.kind_id, stage_id = EXCLUDED.stage_id, topic = EXCLUDED.topic,
    expires_at = EXCLUDED.expires_at, archived_at = EXCLUDED.archived_at,
    archived_reason_id = EXCLUDED.archived_reason_id, agent_id = EXCLUDED.agent_id,
    portable_origin = EXCLUDED.portable_origin, source_id = EXCLUDED.source_id,
    duplicate_of_id = EXCLUDED.duplicate_of_id,
    embedding = NULL, embedding_model = NULL, embedded_at = NULL
WHERE memory.lessons.project_id = EXCLUDED.project_id
RETURNING id
