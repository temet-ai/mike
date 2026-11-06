WITH turns AS (
    SELECT t.id, t.session_id, t.closed_at, t.opened_by, t.user_prompt, t.start_request_id
      FROM memory.operator_turns t
     WHERE t.closed_at IS NOT NULL
       AND t.closed_at >= $1 AND t.closed_at < $2
       AND t.id > $3
     ORDER BY t.id
     LIMIT $4
),
tails AS (
    SELECT tu.id AS turn_id,
           right(string_agg(b.content, '' ORDER BY b.request_id, b.seq_ord), 4000) AS tail
      FROM turns tu
      JOIN requests.requests r ON r.operator_turn_id = tu.id
      JOIN requests.turn_origins o ON o.id = r.turn_origin_id AND o.code NOT LIKE '%sidecar%'
      JOIN responses.stream_blocks b ON b.request_id = r.id
      JOIN responses.stream_block_types bt ON bt.id = b.block_type_id AND bt.code = 'text'
     GROUP BY tu.id
)
SELECT tu.id AS "turn_id!", tu.session_id AS "session_id!", s.session_uuid AS "session_uuid!",
       tu.closed_at AS "closed_at?", tu.opened_by AS "opened_by?", tu.user_prompt AS "user_prompt?",
       sr.claude_code_agent_id AS "agent_id?", COALESCE(tl.tail, '') AS "tail!"
  FROM turns tu
  JOIN sessions.sessions s ON s.id = tu.session_id
  LEFT JOIN requests.requests sr ON sr.id = tu.start_request_id
  LEFT JOIN tails tl ON tl.turn_id = tu.id
 ORDER BY tu.id
