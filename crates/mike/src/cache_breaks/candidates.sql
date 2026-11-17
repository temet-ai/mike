WITH main_origins AS (
    SELECT id FROM requests.turn_origins WHERE code IN ('primary', 'primary_first')
)
SELECT rs.id                                                   AS "response_id!",
       r.id                                                    AS "request_id!",
       r.session_id                                            AS "session_id!",
       r.model,
       r.created_at                                            AS "created_at!",
       CASE WHEN r.last_user_message LIKE '%Mail from %'
              OR r.last_user_message LIKE '%<channel source="mike"%' THEN 'mail'
            ELSE ak.code END                                   AS "woken_by!",
       COALESCE(rs.cache_read_input_tokens, 0)                 AS "cache_read!",
       COALESCE(rs.cache_creation_input_tokens, 0)             AS "cache_creation!",
       COALESCE(rs.input_tokens, 0)                            AS "input!",
       COALESCE((rs.token_usage -> 'cache_creation' ->> 'ephemeral_1h_input_tokens')::bigint, 0) AS "creation_1h!",
       COALESCE((rs.token_usage -> 'cache_creation' ->> 'ephemeral_5m_input_tokens')::bigint, 0) AS "creation_5m!",
       p.id                                                    AS "prev_request_id!",
       p.model                                                 AS prev_model,
       p.completed_at                                          AS "prev_done_at!",
       p.cache_read                                            AS "prev_cache_read!",
       p.cache_creation                                        AS "prev_cache_creation!",
       p.creation_1h                                           AS "prev_creation_1h!",
       p.creation_5m                                           AS "prev_creation_5m!"
  FROM responses.responses rs
  JOIN requests.requests r ON r.id = rs.request_id
  JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
  CROSS JOIN LATERAL (
        SELECT q.id, q.model, qs.completed_at,
               COALESCE(qs.cache_read_input_tokens, 0)     AS cache_read,
               COALESCE(qs.cache_creation_input_tokens, 0) AS cache_creation,
               COALESCE((qs.token_usage -> 'cache_creation' ->> 'ephemeral_1h_input_tokens')::bigint, 0) AS creation_1h,
               COALESCE((qs.token_usage -> 'cache_creation' ->> 'ephemeral_5m_input_tokens')::bigint, 0) AS creation_5m
          FROM requests.requests q
          JOIN responses.responses qs ON qs.request_id = q.id
         WHERE q.session_id = r.session_id
           AND q.created_at < r.created_at
           AND q.turn_origin_id IN (SELECT id FROM main_origins)
           AND q.claude_code_agent_id IS NULL
           AND qs.response_status = 200
         ORDER BY q.created_at DESC, q.id DESC
         LIMIT 1
  ) p
 WHERE rs.id > $1
   AND rs.id <= $2
   AND rs.response_status = 200
   AND rs.cache_creation_input_tokens > $3
   AND r.turn_origin_id IN (SELECT id FROM main_origins)
   AND r.claude_code_agent_id IS NULL
 ORDER BY rs.id
