SELECT ranked.id,
       COALESCE(first_op.user_prompt, first_text.human_prompt) AS originating_operator_prompt
  FROM (SELECT id FROM sessions.sessions ORDER BY last_seen DESC LIMIT 50) ranked
  LEFT JOIN LATERAL (
      SELECT COALESCE(ot.user_prompt, sr.human_prompt) AS user_prompt
        FROM (
            SELECT pk_cls.request_id
              FROM prompt_kinds.classifications pk_cls
             WHERE pk_cls.session_id = ranked.id
               AND pk_cls.kind_id = (
                   SELECT id FROM prompt_kinds.kinds WHERE code = 'operator'
               )
             ORDER BY pk_cls.request_id ASC
             LIMIT 1
        ) first_cls
        JOIN memory.operator_turns ot
          ON ot.start_request_id = first_cls.request_id
        LEFT JOIN requests.requests sr ON sr.id = first_cls.request_id
  ) first_op ON TRUE
  LEFT JOIN LATERAL (
      SELECT r_any.human_prompt
        FROM requests.requests r_any
       WHERE first_op.user_prompt IS NULL
         AND r_any.session_id = ranked.id
         AND r_any.human_prompt IS NOT NULL
         AND r_any.human_prompt <> ''
         AND r_any.turn_origin_id IN (
             SELECT id FROM requests.turn_origins
              WHERE code IN ('primary', 'primary_first', 'subagent', 'workflow_subagent')
         )
       ORDER BY r_any.created_at ASC, r_any.id ASC
       LIMIT 1
  ) first_text ON TRUE
