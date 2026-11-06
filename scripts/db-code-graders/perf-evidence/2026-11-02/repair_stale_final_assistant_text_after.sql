-- A turn's final_assistant_text is a snapshot taken when the turn closes. When the
-- capture transaction holding the turn's last text blocks commits after that
-- snapshot, the turn keeps a NULL or partial final text forever, and the dossier
-- skips it on every pass. This rebuilds the snapshot for every closed turn whose
-- non-sidecar text blocks disagree with it, using the same definition as the close.

WITH block_chars AS (
    SELECT r.operator_turn_id AS turn_id,
           sum(char_length(b.content)) AS chars
      FROM requests.requests r
      JOIN responses.stream_blocks b ON b.request_id = r.id
      JOIN responses.stream_block_types bt ON bt.id = b.block_type_id AND bt.code = 'text'
      JOIN requests.turn_origins o ON o.id = r.turn_origin_id
     WHERE r.operator_turn_id IS NOT NULL
       AND o.code NOT LIKE '%sidecar'
     GROUP BY r.operator_turn_id
), stale AS (
    SELECT t.id
      FROM memory.operator_turns t
      JOIN block_chars c ON c.turn_id = t.id
     WHERE t.closed_at IS NOT NULL
       AND c.chars <> COALESCE(char_length(t.final_assistant_text), 0)
), rebuilt AS (
    SELECT r.operator_turn_id AS turn_id,
           string_agg(b.content, '' ORDER BY b.request_id, b.seq_ord) AS text
      FROM requests.requests r
      JOIN responses.stream_blocks b ON b.request_id = r.id
      JOIN responses.stream_block_types bt ON bt.id = b.block_type_id AND bt.code = 'text'
      JOIN requests.turn_origins o ON o.id = r.turn_origin_id
     WHERE r.operator_turn_id IN (SELECT id FROM stale)
       AND o.code NOT LIKE '%sidecar'
     GROUP BY r.operator_turn_id
)
UPDATE memory.operator_turns t
   SET final_assistant_text = rebuilt.text
  FROM rebuilt
 WHERE t.id = rebuilt.turn_id
   AND t.final_assistant_text IS DISTINCT FROM rebuilt.text;
