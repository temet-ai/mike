SELECT r.id AS request_id,
       r.created_at AS at,
       string_agg(b.content, E'\n\n' ORDER BY b.seq_ord) AS text
  FROM requests.requests r
  JOIN prompt_kinds.classifications c ON c.request_id = r.id
  JOIN prompt_kinds.kinds k ON k.id = c.kind_id AND k.code = 'session-recap'
  JOIN responses.stream_blocks b ON b.request_id = r.id
  JOIN responses.stream_block_types bt ON bt.id = b.block_type_id AND bt.code = 'text'
 WHERE r.session_id = $1
   AND r.created_at <= $2
   AND r.claude_code_agent_id IS NULL
   AND btrim(b.content) <> ''
 GROUP BY r.id, r.created_at
 ORDER BY r.created_at, r.id
