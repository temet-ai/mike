WITH due AS (
    SELECT f.id, f.delivered_request_id
      FROM workspace.code_findings f
     WHERE f.delivered_request_id IS NOT NULL
       AND f.reply_at IS NULL
       AND f.delivered_at < now() - interval '1 minute'
     ORDER BY f.delivered_at
     LIMIT 100
), answered AS (
    SELECT due.id, t.text
      FROM due
      LEFT JOIN requests.requests r ON r.id = due.delivered_request_id
      LEFT JOIN LATERAL (
          SELECT string_agg(b.content, '' ORDER BY b.seq_ord) AS text
            FROM responses.stream_blocks b
            JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
           WHERE b.request_id = due.delivered_request_id AND bt.code = 'text'
             AND r.turn_origin_id IN (SELECT o.id FROM requests.turn_origins o
                                       WHERE o.code IN ('primary', 'primary_first', 'subagent', 'workflow_subagent'))
      ) t ON true
)
UPDATE workspace.code_findings f
   SET reply = a.text,
       reply_at = now()
  FROM answered a
 WHERE f.id = a.id
