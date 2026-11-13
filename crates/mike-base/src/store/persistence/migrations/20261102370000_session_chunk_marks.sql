-- The newest chunk embedding time per session, kept on one row per session so the session
-- embedding sweep reads one narrow row per session instead of aggregating every chunk joined
-- to every turn on each pass. A statement-level trigger keeps it current for every writer of
-- operator_turn_chunks; the value only moves forward, and the sweep reads the same value back
-- when it records source_updated_at, so a mark left ahead of a deleted chunk costs at most one
-- re-embed and never a loop.
CREATE TABLE IF NOT EXISTS memory.session_chunk_marks (
    session_id         bigint      PRIMARY KEY REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    chunks_embedded_at timestamptz NOT NULL
);

CREATE OR REPLACE FUNCTION memory.mark_session_chunks_embedded() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO memory.session_chunk_marks (session_id, chunks_embedded_at)
    SELECT ot.session_id, max(nc.embedded_at)
      FROM new_chunks nc
      JOIN memory.operator_turns ot ON ot.id = nc.turn_id
     GROUP BY ot.session_id
    ON CONFLICT (session_id) DO UPDATE
       SET chunks_embedded_at = EXCLUDED.chunks_embedded_at
     WHERE memory.session_chunk_marks.chunks_embedded_at < EXCLUDED.chunks_embedded_at;
    RETURN NULL;
END;
$$;

DROP TRIGGER IF EXISTS trg_operator_turn_chunks_mark_session ON memory.operator_turn_chunks;
CREATE TRIGGER trg_operator_turn_chunks_mark_session
    AFTER INSERT ON memory.operator_turn_chunks
    REFERENCING NEW TABLE AS new_chunks
    FOR EACH STATEMENT EXECUTE FUNCTION memory.mark_session_chunks_embedded();

INSERT INTO memory.session_chunk_marks (session_id, chunks_embedded_at)
SELECT ot.session_id, max(otc.embedded_at)
  FROM memory.operator_turn_chunks otc
  JOIN memory.operator_turns ot ON ot.id = otc.turn_id
 GROUP BY ot.session_id
ON CONFLICT (session_id) DO UPDATE
   SET chunks_embedded_at = EXCLUDED.chunks_embedded_at
 WHERE memory.session_chunk_marks.chunks_embedded_at < EXCLUDED.chunks_embedded_at;
