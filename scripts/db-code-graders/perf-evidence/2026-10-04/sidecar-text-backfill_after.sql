WITH affected AS (
    SELECT DISTINCT r.operator_turn_id AS turn_id
    FROM requests.requests r
    JOIN requests.turn_origins o ON o.id = r.turn_origin_id AND o.code LIKE '%sidecar'
    JOIN responses.stream_blocks b ON b.request_id = r.id
    JOIN responses.stream_block_types bt ON bt.id = b.block_type_id AND bt.code = 'text'
    WHERE r.operator_turn_id IS NOT NULL
      AND r.created_at > now() - interval '14 days'
), texts AS (
    SELECT a.turn_id, string_agg(b.content, '' ORDER BY b.request_id, b.seq_ord) AS content
    FROM affected a
    JOIN requests.requests r ON r.operator_turn_id = a.turn_id
    JOIN requests.turn_origins o ON o.id = r.turn_origin_id AND o.code NOT LIKE '%sidecar'
    JOIN responses.stream_blocks b ON b.request_id = r.id
    JOIN responses.stream_block_types bt ON bt.id = b.block_type_id AND bt.code = 'text'
    GROUP BY a.turn_id
)
UPDATE memory.operator_turns t
   SET final_assistant_text = x.content
  FROM (SELECT a.turn_id, tx.content
          FROM affected a LEFT JOIN texts tx ON tx.turn_id = a.turn_id) x
 WHERE t.id = x.turn_id
   AND t.closed_at IS NOT NULL
   AND t.final_assistant_text IS DISTINCT FROM x.content;
