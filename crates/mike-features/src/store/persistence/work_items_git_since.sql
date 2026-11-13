WITH k AS (
    SELECT * FROM unnest($1::bigint[], $2::timestamptz[]) AS k(session_id, after)
)
SELECT res.id AS "result_id!",
       carrier.session_id AS "session_id!",
       r.claude_code_agent_id AS "agent_id?",
       res.created_at AS "at!",
       g.lines AS "text!"
  FROM k
  JOIN requests.requests carrier
    ON carrier.session_id = k.session_id AND carrier.created_at > k.after
  JOIN tools.results res ON res.request_id = carrier.id
  JOIN tools.uses u ON u.id = res.tool_use_id
  JOIN requests.requests r ON r.id = u.request_id
 CROSS JOIN LATERAL (
      SELECT CASE jsonb_typeof(res.content)
                 WHEN 'string' THEN res.content #>> '{}'
                 ELSE string_agg(e.item->>'text', E'\n' ORDER BY e.n)
                          FILTER (WHERE jsonb_typeof(e.item->'text') = 'string')
             END AS text
        FROM jsonb_array_elements(
                 CASE jsonb_typeof(res.content) WHEN 'array' THEN res.content
                                                ELSE '[]'::jsonb END
             ) WITH ORDINALITY AS e(item, n)
 ) body
 CROSS JOIN LATERAL (
      SELECT coalesce(string_agg(t.line, E'\n' ORDER BY t.n), '') AS lines
        FROM regexp_split_to_table(coalesce(body.text, ''), E'\n')
             WITH ORDINALITY AS t(line, n)
       WHERE t.line LIKE '% -> %' OR t.line ~ '^\s*\['
 ) g
 WHERE u.tool_name = 'Bash'
   AND (u.input->>'command' LIKE '%git push%' OR u.input->>'command' LIKE '%git commit%')
