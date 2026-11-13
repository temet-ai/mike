INSERT INTO memory.semantic_items
    (kind, session_id, operator_turn_id, project_id, request_id, origin_session_uuid, origin_turn_id,
     source_ref, pattern, text, text_chars, embedding, embedder_key, vector_source, builder_version,
     effective_at, data)
SELECT 'anchor', NULL, NULL, m.project_id, m.request_id, s.session_uuid, m.operator_turn_id,
       'memory.pattern_candidates:' || c.id || ':' || m.id, $2, m.text, m.text_chars, m.embedding,
       m.embedder_key, 'member_copy', $3, m.effective_at, jsonb_build_object('candidate_id', c.id)
  FROM memory.pattern_candidates c
  JOIN memory.semantic_items m ON m.id = ANY(c.member_item_ids)
  JOIN sessions.sessions s ON s.id = m.session_id
 WHERE c.id = $1 AND m.operator_turn_id IS NOT NULL
ON CONFLICT (kind, source_ref, embedder_key, builder_version) DO NOTHING
RETURNING id
