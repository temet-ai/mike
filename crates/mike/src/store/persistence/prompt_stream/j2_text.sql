SELECT msgs.session_id AS "session_id!",
       msgs.request_id AS "request_id!",
       msgs.created_at AS "created_at!",
       msgs.turn_id AS "turn_id?",
       NULL::text AS "kind_code?",
       NULL::text AS "rule?",
       NULL::text AS "repeat_key?",
       NULL::text AS "classifier_version?",
       msgs.text AS "text?",
       msgs.author AS "author?",
       msgs.has_agent_id AS "has_agent_id!"
  FROM (
       SELECT t.session_id, t.start_request_id AS request_id, sr.created_at,
              t.id AS turn_id, t.user_prompt AS text, ak.code AS author,
              (sr.claude_code_agent_id IS NOT NULL) AS has_agent_id
         FROM memory.operator_turns t
         JOIN requests.requests sr ON sr.id = t.start_request_id
         LEFT JOIN requests.author_kinds ak ON ak.id = sr.author_kind_id
        WHERE t.session_id = ANY($1)
          AND sr.created_at >= $2
          AND sr.created_at <= $3
       UNION ALL
       SELECT r.session_id, r.id, r.created_at,
              NULL::bigint, r.human_prompt, ak.code,
              (r.claude_code_agent_id IS NOT NULL)
         FROM requests.requests r
         JOIN requests.turn_types tt ON tt.id = r.turn_type_id
         JOIN requests.turn_origins o ON o.id = r.turn_origin_id
         LEFT JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
        WHERE r.session_id = ANY($1)
          AND r.created_at >= $2
          AND r.created_at <= $3
          AND tt.code = 'user_prompt'
          AND o.code IN ('primary', 'primary_first')
          AND r.operator_turn_id IS NULL
          AND r.human_prompt <> ''
       ) msgs
 ORDER BY msgs.session_id, msgs.request_id
