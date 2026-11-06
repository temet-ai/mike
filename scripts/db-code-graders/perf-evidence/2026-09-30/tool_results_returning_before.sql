INSERT INTO tools.results (request_id, ord, tool_use_id, is_error, content, content_sha256, content_bytes, created_at, session_id)
SELECT u.req_id, u.ord, tu.id, u.is_err, u.content, u.sha, u.bytes, u.req_at, 1313616::bigint
FROM UNNEST(ARRAY[488229]::bigint[], ARRAY['2026-09-30 13:29:12+00']::timestamptz[], ARRAY[0]::int[], ARRAY['toolu_014rbfc1dhaAdEgu6TsySddE']::text[], ARRAY[false]::bool[], ARRAY['"x"'::jsonb], ARRAY['sha']::text[], ARRAY[1]::bigint[]) AS u(req_id, req_at, ord, tool_use_id, is_err, content, sha, bytes)
JOIN tools.uses tu ON tu.wire_tool_use_id = u.tool_use_id
ON CONFLICT (tool_use_id) DO NOTHING;
