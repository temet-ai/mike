-- GitHub issue #102: a failed responses.stream_blocks INSERT was WARN-
-- swallowed with no terminal retry and no queryable trace, so a dropped
-- block silently produced an incomplete reconstructed response (raw SSE
-- bytes are never stored, so the loss was unrecoverable). The write path
-- now buffers a failed block and retries it in insert_request_initial's
-- terminal ENSURE pass, mirroring the tool_calls crash-recovery net; this
-- column is the queryable signal for the case the retry still can't close
-- the gap (or the entry evicted from the ring before the retry ran — D7).
--
-- Constant default on a boolean column is a metadata-only change on
-- Postgres 11+ (no table rewrite), so this is safe on the live table.
ALTER TABLE responses.responses
    ADD COLUMN IF NOT EXISTS stream_blocks_gapped boolean NOT NULL DEFAULT false;

COMMENT ON COLUMN responses.responses.stream_blocks_gapped IS
    'True when a streaming response''s committed responses.stream_blocks '
    'row count is short of the block count the updater''s BlockAccumulator '
    'actually closed for this request, after the terminal ENSURE retry '
    'ran. False for every non-streaming response and every streaming '
    'response with full block coverage. A distinguishable, queryable '
    'incompleteness signal — never infer completeness from silence.';
