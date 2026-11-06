SELECT r.id AS request_id, r.session_id,
        r.human_prompt, r.last_user_message,
        COALESCE(ak.code, 'operator') AS author,
        (r.claude_code_agent_id IS NOT NULL) AS has_agent_id
   FROM requests.requests r
   JOIN requests.turn_types tt ON tt.id = r.turn_type_id AND tt.code = 'user_prompt'
   JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                               AND o.code IN ('primary', 'primary_first')
   LEFT JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
   LEFT JOIN prompt_kinds.classifications c ON c.request_id = r.id
  WHERE r.session_id = ANY(ARRAY[1202255,1202152,1173418,1139628,1158718,1173693,1117541,1096395,1035456,1035527,1035386,1050117,1035156,1047553,1034189,981647,1035047,981652,956801,931197,933623,947779,925594,934506,934158]::bigint[])
    AND (c.id IS NULL OR c.classifier_version IS DISTINCT FROM 'genuine-v1')
  ORDER BY r.id
