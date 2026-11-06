INSERT INTO prompt_kinds.classifications (request_id, session_id, kind_id, rule, repeat_key, classifier_version)
SELECT x.request_id, x.session_id, k.id, x.rule, x.repeat_key, x.version
  FROM jsonb_to_recordset((SELECT jsonb_agg(jsonb_build_object('request_id', request_id, 'session_id', session_id, 'kind', 'operator', 'rule', NULL, 'repeat_key', NULL, 'version', 'genuine-v1')) FROM (SELECT r.id AS request_id, r.session_id FROM requests.requests r LEFT JOIN prompt_kinds.classifications c ON c.request_id = r.id WHERE c.id IS NULL ORDER BY r.id LIMIT 2000) q))
       AS x(request_id bigint, session_id bigint, kind text, rule text, repeat_key text, version text)
  JOIN prompt_kinds.kinds k ON k.code = x.kind
ON CONFLICT (request_id) DO NOTHING
