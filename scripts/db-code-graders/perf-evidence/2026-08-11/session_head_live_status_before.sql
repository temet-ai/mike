SELECT
     s.session_uuid::text AS session_id,
     s.last_status        AS last_status,
     newest.id            AS newest_request_id,
     newest.rn            AS newest_request_number,
     first_op.user_prompt AS originating_operator_prompt
 FROM sessions.sessions s
 LEFT JOIN LATERAL (
     SELECT r.id,
            ROW_NUMBER() OVER (
                PARTITION BY r.session_id
                ORDER BY r.created_at ASC, r.id ASC
            ) AS rn
     FROM requests.requests r
     WHERE r.session_id = 721059
     ORDER BY r.created_at DESC, r.id DESC
     LIMIT 1
 ) newest ON TRUE
 LEFT JOIN LATERAL (
     SELECT COALESCE(ot.user_prompt, sr.human_prompt) AS user_prompt
       FROM (
           SELECT pk_cls.request_id
             FROM prompt_kinds.classifications pk_cls
            WHERE pk_cls.session_id = 721059
              AND pk_cls.kind_id = (SELECT id FROM prompt_kinds.kinds WHERE code = 'operator')
            ORDER BY pk_cls.request_id ASC
            LIMIT 1
       ) first_cls
       JOIN memory.operator_turns ot ON ot.start_request_id = first_cls.request_id
       LEFT JOIN requests.requests sr ON sr.id = first_cls.request_id
 ) first_op ON TRUE
 WHERE s.id = 721059;
