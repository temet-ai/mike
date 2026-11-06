UPDATE memory.operator_turns t
   SET final_assistant_text = s.text
  FROM (
      SELECT r.operator_turn_id AS turn_id,
             string_agg(b.content, '' ORDER BY b.request_id, b.seq_ord) AS text
        FROM requests.requests r
        JOIN responses.stream_blocks b ON b.request_id = r.id
        JOIN requests.turn_origins o ON o.id = r.turn_origin_id
        JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
       WHERE r.operator_turn_id = ANY(ARRAY[48716,48717,48787,48792,48793,48822,48871,48937]::bigint[])
         AND bt.code = 'text'
         AND o.code NOT LIKE '%sidecar'
       GROUP BY r.operator_turn_id
  ) s
 WHERE t.id = s.turn_id
   AND t.final_assistant_text IS DISTINCT FROM s.text
