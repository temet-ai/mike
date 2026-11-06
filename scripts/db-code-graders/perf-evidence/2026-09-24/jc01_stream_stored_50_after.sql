SELECT c.session_id, r.id AS request_id, r.created_at, t.id AS turn_id, k.code AS kind_code, c.rule, c.repeat_key, c.classifier_version,
       NULL::text AS text, NULL::text AS author, (r.claude_code_agent_id IS NOT NULL) AS has_agent_id
  FROM prompt_kinds.classifications c
  JOIN prompt_kinds.kinds k ON k.id = c.kind_id
  JOIN requests.requests r ON r.id = c.request_id
  LEFT JOIN memory.operator_turns t ON t.start_request_id = r.id
 WHERE c.session_id = ANY(ARRAY[1173418,809495,1139628,912552,1035386,1035156,729446,798097,1202255,797875,956801,639520,1202152,925594,918604,912780,1117541,775485,1096395,1035527,1035456,931197,836711,1158718,918521,683406,947779,1034189,737382,799253,933623,981652,934506,723785,652849,912733,1173693,1035047,633696,981647,816099,1047553,912857,934158,810319,814115,814539,720077,1050117,868867]::bigint[]) AND r.created_at >= '2026-07-27' AND r.created_at <= now() AND k.code <> 'empty-prompt'
 ORDER BY c.session_id, r.id
