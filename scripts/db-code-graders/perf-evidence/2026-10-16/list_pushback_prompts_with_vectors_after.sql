SELECT t.id AS turn_id,
       s.session_uuid,
       t.created_at,
       t.project_id,
       t.user_prompt AS text,
       c.chunk_embedding::real[] AS embedding
  FROM memory.operator_turns t
  JOIN memory.turn_facts f ON f.operator_turn_id = t.id
  JOIN sessions.sessions s ON s.id = t.session_id
  JOIN memory.operator_turn_chunks c ON c.turn_id = t.id AND c.chunk_idx = 0
  JOIN memory.chunk_roles r ON r.id = c.chunk_role_id AND r.code = 'prompt'
 WHERE t.created_at >= '2026-07-24T00:00:00Z'::timestamptz
   AND f.prompt_kind = 'genuine'
   AND t.reaction IN ('corrected', 'redirected')
   AND (t.subject_agent IS NULL OR t.subject_agent = 'main')
   AND t.user_prompt IS NOT NULL
   AND length(t.user_prompt) BETWEEN 20 AND 1500
   AND t.user_prompt !~ '^[A-Za-z0-9_-]+\. Your (brief|handover) is saved at'
   AND t.user_prompt !~ '^<session>'
   AND t.user_prompt NOT LIKE '%[Image: source:%'
   AND t.user_prompt NOT LIKE '%[Request interrupted by user]%'
