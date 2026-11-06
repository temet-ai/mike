-- Re-key `overrides.overrides` from `prompt_sha256` (a per-request nonce —
-- Claude Code embeds a live git-status block in the hashed text, so 82.6% of
-- captured shas occur exactly once) onto `(lane, agent_name)`, the stable
-- class identity already available pre-filter via `RequestSignals`. See
-- artifacts/threads/prompts-surface-rebuild/backend-blueprint.md, the
-- STOP-THE-LINE section and its Unit 3 migration detail.
--
-- Five parts, in dependency order:
--   1. Add `lane` (mirrors system_prompts.session_prompts.lane's two-value
--      vocabulary exactly -- verified live: only 'main'/'subagent' occur
--      there, 1243/2638 rows -- not the 11-code requests.turn_origins set,
--      which is request-grain, not registration-grain).
--   2. Dedupe existing MANUAL registrations onto the new key, keeping the
--      newest per key. Required before step 4: `create_composition` has no
--      idempotency guard against re-registering the same class (separately
--      noted, not fixed here), which produced 32 duplicate 'steer-author'
--      rows (verified live: same agent_name, same blocks payload, version=1
--      throughout, zero requests.system_prompts.override_id references --
--      confirmed safe to collapse, nothing points at the deleted ids). That
--      FK is plain NO ACTION (not CASCADE, unlike overrides.revisions), so a
--      referenced duplicate would abort the whole migration with a raw FK
--      violation rather than the app's friendlier 409 StillReferenced check
--      (compositions.rs) -- a guard below turns that into a clear RAISE
--      instead, so a startup failure here is never a cryptic constraint
--      trace.
--   3. Demote `prompt_sha256` to nullable -- no single sha represents a
--      class under the new key.
--   4. Two partial-unique indexes on the real key, scoped to
--      source_id=1 ('manual' -- filter-seeded stubs, source_id=2, never
--      match either predicate and need no ordering against their removal
--      in a separate unit): one per subagent persona, one singleton for
--      main (which has no persona to key on). Uniquely indexing this key is
--      safe under .claude/rules/sql-set-based.md -- the ban targets content
--      hashes (sha), not a stable identity column.
--   5. Drop the pre-existing plain btree index on `prompt_sha256` (baseline,
--      `20260604000001_baseline.sql:790`) -- now that the column is
--      explicitly informational, this index is exactly the shape
--      .claude/rules/sql-set-based.md bans. Measured live before dropping:
--      table is 66 rows / 248 kB; EXPLAIN (ANALYZE, BUFFERS) on a sha miss
--      showed 0.033ms execution against a 0.629ms planning cost -- a seq
--      scan at this size is free, so `GET /api/prompts/compositions/
--      by-sha256/{prompt_sha256}` (compositions.rs:178, a plain equality
--      lookup with no index-name dependency -- Postgres has no index
--      hints) changes access path only, not result or correctness.
--
-- Idempotent: IF NOT EXISTS / DROP CONSTRAINT IF EXISTS / DROP INDEX IF
-- EXISTS; the dedup DELETEs are no-ops once no duplicates remain; DROP NOT
-- NULL is a no-op if already nullable.

-- ─── 1. lane column ─────────────────────────────────────────────────────

ALTER TABLE overrides.overrides
    ADD COLUMN IF NOT EXISTS lane text NOT NULL DEFAULT 'main';

UPDATE overrides.overrides SET lane = 'subagent' WHERE agent_name IS NOT NULL;

ALTER TABLE overrides.overrides
    DROP CONSTRAINT IF EXISTS overrides_lane_check;

ALTER TABLE overrides.overrides
    ADD CONSTRAINT overrides_lane_check
        CHECK (lane IN ('main', 'subagent'));

-- ─── 2. Dedupe manual registrations onto (lane, agent_name) ────────────
-- Set-based rank + delete (no correlated subquery): for each key, keep the
-- row with the latest updated_at (ties broken by highest id), delete the
-- rest. Revisions cascade-delete with their own override row (FK ON DELETE
-- CASCADE) -- each duplicate here has exactly one revision of its own, none
-- shared, so nothing is orphaned.

-- Guard: verified live that zero requests.system_prompts rows reference any
-- duplicate id in either bucket, and structurally none can gain a reference
-- before this migration ships (the composer is still sha-keyed today and
-- has already proven it never matches these rows -- that sha volatility is
-- this whole redesign's premise). Checked anyway, set-based, so a violated
-- assumption fails loudly here instead of as a raw FK-violation trace during
-- the DELETE below.
DO $$
DECLARE
    still_referenced bigint;
BEGIN
    SELECT count(*) INTO still_referenced
    FROM requests.system_prompts rsp
    JOIN (
        SELECT id, row_number() OVER (
                   PARTITION BY agent_name
                   ORDER BY updated_at DESC, id DESC
               ) AS rn
        FROM overrides.overrides
        WHERE source_id = 1 AND lane = 'subagent' AND agent_name IS NOT NULL
        UNION ALL
        SELECT id, row_number() OVER (
                   ORDER BY updated_at DESC, id DESC
               ) AS rn
        FROM overrides.overrides
        WHERE source_id = 1 AND lane = 'main'
    ) dup ON dup.id = rsp.override_id AND dup.rn > 1;

    IF still_referenced > 0 THEN
        RAISE EXCEPTION
            'overrides.overrides dedupe: % duplicate row(s) slated for removal are still referenced by requests.system_prompts.override_id -- migration aborted, no rows touched, needs a manual dedupe decision',
            still_referenced;
    END IF;
END $$;

WITH ranked AS (
    SELECT id,
           row_number() OVER (
               PARTITION BY agent_name
               ORDER BY updated_at DESC, id DESC
           ) AS rn
    FROM overrides.overrides
    WHERE source_id = 1 AND lane = 'subagent' AND agent_name IS NOT NULL
)
DELETE FROM overrides.overrides
WHERE id IN (SELECT id FROM ranked WHERE rn > 1);

WITH ranked AS (
    SELECT id,
           row_number() OVER (
               ORDER BY updated_at DESC, id DESC
           ) AS rn
    FROM overrides.overrides
    WHERE source_id = 1 AND lane = 'main'
)
DELETE FROM overrides.overrides
WHERE id IN (SELECT id FROM ranked WHERE rn > 1);

-- ─── 3. prompt_sha256 becomes informational, not a lookup key ──────────

ALTER TABLE overrides.overrides
    ALTER COLUMN prompt_sha256 DROP NOT NULL;

-- ─── 4. Partial-unique indexes on the real key ─────────────────────────

CREATE UNIQUE INDEX IF NOT EXISTS uq_overrides_manual_subagent_persona
    ON overrides.overrides (agent_name)
    WHERE lane = 'subagent' AND source_id = 1;

CREATE UNIQUE INDEX IF NOT EXISTS uq_overrides_manual_main_singleton
    ON overrides.overrides ((true))
    WHERE lane = 'main' AND source_id = 1;

-- ─── 5. Drop the legacy sha index -- column is informational now ───────

DROP INDEX IF EXISTS overrides.idx_overrides_prompt_sha256;
