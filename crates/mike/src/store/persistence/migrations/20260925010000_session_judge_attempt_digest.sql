-- session_judges.attempts.last_digest_sha256: the digest the session judge last judged successfully.
--
-- Why. Between 2026-09-02 and 2026-09-21 llm.calls holds 34,318 ok session_judge calls over only
-- 393 distinct digests (mike#324): the same bytes were sent to the model thousands of times.
-- a4f40c45 stopped the selection loop that caused it, but nothing stops a call whose digest is
-- byte-identical to the one already judged, so any future selection defect repeats the waste.
-- The legacy session judge stores SHA-256(system prompt, digest) here after every successful call
-- and, before the next call, skips the model when the new digest hashes the same, moving only
-- last_attempt_at forward so the selection waits for a newer turn.
--
-- Expand-only: one nullable column, catalogue-only on PostgreSQL 11 and later.

ALTER TABLE session_judges.attempts
    ADD COLUMN IF NOT EXISTS last_digest_sha256 bytea NULL;

COMMENT ON COLUMN session_judges.attempts.last_digest_sha256 IS
    'SHA-256 of the system prompt and digest the last successful call for this (session, '
    'judge_model, rubric_version) judged. NULL until one succeeds after this column was added. '
    'Written by record_judged_digest; a matching digest makes skip_repeated_digest skip the call.';
