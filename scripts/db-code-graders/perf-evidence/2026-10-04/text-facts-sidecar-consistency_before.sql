WITH blk AS (
    SELECT r.operator_turn_id AS turn_id, b.request_id, b.seq_ord, o.code AS origin,
           COALESCE(char_length(b.content), 0) AS n,
           sum(COALESCE(char_length(b.content), 0)) OVER w AS end_off,
           row_number() OVER (PARTITION BY r.operator_turn_id
                              ORDER BY b.request_id DESC, b.seq_ord DESC) AS rn_desc,
           CASE WHEN o.code = lag(o.code) OVER w THEN 0 ELSE 1 END AS run_start
      FROM requests.requests r
      JOIN responses.stream_blocks b ON b.request_id = r.id
      JOIN responses.stream_block_types bt ON bt.id = b.block_type_id AND bt.code = 'text'
      JOIN requests.turn_origins o ON o.id = r.turn_origin_id
     WHERE r.operator_turn_id = ANY(ARRAY[17963,18000,24640]::bigint[])
    WINDOW w AS (PARTITION BY r.operator_turn_id ORDER BY b.request_id, b.seq_ord)
), runs AS (
    SELECT blk.*,
           sum(run_start) OVER (PARTITION BY turn_id ORDER BY request_id, seq_ord) AS run_no
      FROM blk
), spans AS (
    SELECT turn_id, run_no, origin,
           min(end_off - n) AS start_off, max(end_off) AS end_off,
           min(request_id) AS first_request_id, max(request_id) AS last_request_id
      FROM runs
     GROUP BY turn_id, run_no, origin
), span_json AS (
    SELECT turn_id,
           jsonb_agg(jsonb_build_object(
               'origin', origin, 'start', start_off, 'end', end_off,
               'first_request_id', first_request_id, 'last_request_id', last_request_id
           ) ORDER BY start_off) AS spans
      FROM spans
     GROUP BY turn_id
), agg AS (
    SELECT turn_id,
           max(end_off) AS text_chars,
           COALESCE(max(end_off) FILTER (WHERE origin NOT LIKE '%sidecar'), 0) AS text_cut_at,
           max(origin) FILTER (WHERE rn_desc = 1) AS last_text_origin
      FROM runs
     GROUP BY turn_id
)
SELECT t.id AS turn_id,
       COALESCE(agg.text_chars, 0)::int AS text_chars,
       COALESCE(agg.text_cut_at, 0)::int AS text_cut_at,
       agg.last_text_origin,
       COALESCE(span_json.spans, '[]'::jsonb) AS text_spans,
       (COALESCE(agg.text_chars, 0) = COALESCE(char_length(t.final_assistant_text), 0)) AS consistent
  FROM memory.operator_turns t
  LEFT JOIN agg ON agg.turn_id = t.id
  LEFT JOIN span_json ON span_json.turn_id = t.id
 WHERE t.id = ANY(ARRAY[17963,18000,24640]::bigint[])
 ORDER BY t.id;
