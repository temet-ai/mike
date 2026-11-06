UPDATE prompt_kinds.classifications c
   SET kind_id = k.id, rule = x.rule, repeat_key = x.repeat_key, classifier_version = x.version
  FROM jsonb_to_recordset((SELECT jsonb_agg(jsonb_build_object('request_id', request_id, 'session_id', session_id, 'kind', 'loop-wakeup', 'rule', 'a', 'repeat_key', 'k', 'version', 'genuine-v1')) FROM (SELECT request_id, session_id FROM prompt_kinds.classifications ORDER BY id LIMIT 2000) q))
       AS x(request_id bigint, session_id bigint, kind text, rule text, repeat_key text, version text)
  JOIN prompt_kinds.kinds k ON k.code = x.kind
 WHERE c.request_id = x.request_id
   AND (c.kind_id, c.rule, c.repeat_key, c.classifier_version) IS DISTINCT FROM (k.id, x.rule, x.repeat_key, x.version)
