SELECT msgs.session_id,
        msgs.request_id,
        msgs.created_at,
        msgs.turn_id,
        NULL::text AS kind_code,
        NULL::text AS rule,
        NULL::text AS repeat_key,
        NULL::text AS classifier_version,
        msgs.text,
        msgs.author,
        msgs.has_agent_id
   FROM (
        SELECT t.session_id, t.start_request_id AS request_id, sr.created_at,
               t.id AS turn_id, t.user_prompt AS text, ak.code AS author,
               (sr.claude_code_agent_id IS NOT NULL) AS has_agent_id
          FROM memory.operator_turns t
          JOIN requests.requests sr ON sr.id = t.start_request_id
          LEFT JOIN requests.author_kinds ak ON ak.id = sr.author_kind_id
         WHERE t.session_id = ANY(ARRAY[1173418,809495,1139628,912552,1035386,1035156,729446,798097,1202255,797875,956801,639520,1202152,925594,918604,912780,1117541,775485,1096395,1035527,1035456,931197,836711,1158718,918521,683406,947779,1034189,737382,799253,933623,981652,934506,723785,652849,912733,1173693,1035047,633696,981647,816099,1047553,912857,934158,810319,814115,814539,720077,1050117,868867]::bigint[])
           AND sr.created_at >= '2026-07-27'::timestamptz
           AND sr.created_at <= now()
        UNION ALL
        SELECT r.session_id, r.id, r.created_at,
               NULL::bigint, r.human_prompt, ak.code,
               (r.claude_code_agent_id IS NOT NULL)
          FROM requests.requests r
          JOIN requests.turn_types tt ON tt.id = r.turn_type_id
          JOIN requests.turn_origins o ON o.id = r.turn_origin_id
          LEFT JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
         WHERE r.session_id = ANY(ARRAY[1173418,809495,1139628,912552,1035386,1035156,729446,798097,1202255,797875,956801,639520,1202152,925594,918604,912780,1117541,775485,1096395,1035527,1035456,931197,836711,1158718,918521,683406,947779,1034189,737382,799253,933623,981652,934506,723785,652849,912733,1173693,1035047,633696,981647,816099,1047553,912857,934158,810319,814115,814539,720077,1050117,868867]::bigint[])
           AND r.created_at >= '2026-07-27'::timestamptz
           AND r.created_at <= now()
           AND tt.code = 'user_prompt'
           AND o.code IN ('primary', 'primary_first')
           AND r.operator_turn_id IS NULL
           AND r.human_prompt <> ''
        ) msgs
  ORDER BY msgs.session_id, msgs.request_id