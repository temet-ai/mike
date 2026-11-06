-- Add the full outbound request body to llm.calls — additive, idempotent.
--
-- llm.calls (20260829000002_llm_calls.sql) records cost and tokens for
-- every outbound OpenRouter call but never the prompt itself, so the
-- session judge's related-sessions block — the neighbour-session context
-- spliced into the judge prompt, built by
-- `judge_worker::build_related_sessions_block` — could not be read back
-- off the live system after the fact; it only ever existed on the wire.
-- This column closes that gap: the exact JSON body sent to OpenRouter
-- (system + messages + parameters), captured once by the client
-- (`crates/mike/src/llm/openrouter.rs`) and stored verbatim, never rebuilt
-- or sliced.
--
-- Nullable: rows inserted before this migration ran have no value here,
-- and a call refused by the ceiling/rate/spend limiter never reaches
-- `build_body` at all — there is nothing to store for those, not a
-- truncated value.
--
-- No index, matching `llm.calls.generation_id`'s precedent: this column
-- is for investigative reads of one call at a time, never for searching
-- or filtering by content.

ALTER TABLE llm.calls ADD COLUMN IF NOT EXISTS request_body jsonb NULL;

COMMENT ON COLUMN llm.calls.request_body IS
    'Full outbound request body (system + messages + parameters) as sent '
    'to OpenRouter, captured once by the client and stored unchanged. '
    'NULL on rows predating this column and on refused calls, which never '
    'build a body.';
