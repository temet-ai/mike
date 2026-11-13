-- Auto-enrol steerers: make the generic fleet auto-enrol into every new
-- session at session start, replacing the enrolment-free special path.
--
-- EXPAND ONLY — no columns removed, no NOT NULL tightened, no indexes dropped.
-- All changes are additive or nullable additions.
--
-- Idempotency:
--   - ADD COLUMN IF NOT EXISTS is a no-op on re-run.
--   - UPDATE … WHERE kind = 'false_done' is idempotent (sets true to true).
--   - CREATE INDEX IF NOT EXISTS is a no-op on re-run.
--   - FK on fired_response_id: ALTER TABLE … ADD COLUMN IF NOT EXISTS is safe.

-- ─── 1. Add auto_enrol flag to steering.steerers ─────────────────────────────
-- true => auto-enrolled into every new session at session start (the generic
-- replacement for the false_done always-on special case).

ALTER TABLE steering.steerers
    ADD COLUMN IF NOT EXISTS auto_enrol bool NOT NULL DEFAULT false;

COMMENT ON COLUMN steering.steerers.auto_enrol IS
    'true => auto-enrolled into every new session at session start '
    '(the generic replacement for the false_done always-on special case).';

-- ─── 2. Seed false_done steerer auto_enrol = true ─────────────────────────────
-- Idempotent: SET true ON true is a no-op at the data level.
-- refocus and memory_keeper remain false; the operator marks them later.

UPDATE steering.steerers
   SET auto_enrol = true,
       updated_at = NOW()
 WHERE kind = 'false_done';

-- ─── 3. Add fired_response_id to steering.interventions ──────────────────────
-- trigger_request_id covers the request that the steerer evaluated.
-- sent_response_id covers the response to the delivered intervention request.
-- There is no column for the ASSISTANT RESPONSE that was *judged* by the
-- steerer (the response produced by the model in reaction to trigger_request_id).
-- fired_response_id fills that gap.
--
-- Nullable: populated by the generic fleet on each evaluation event; legacy
-- false_done rows from the migration in 20260629000002 will be NULL here.
-- ON DELETE SET NULL: if the response row is evicted the intervention is kept.

ALTER TABLE steering.interventions
    ADD COLUMN IF NOT EXISTS fired_response_id bigint
        REFERENCES responses.responses(id) ON DELETE SET NULL;

COMMENT ON COLUMN steering.interventions.fired_response_id IS
    'The assistant response that was evaluated by the steerer '
    '(i.e. the response to trigger_request_id). '
    'Distinct from sent_response_id, which is the response to the delivered '
    'intervention request. NULL for legacy rows migrated from false_done_fires.';

-- Index the FK (required by schema conventions; SET NULL makes partial index
-- on IS NOT NULL cleaner than full-table index once most rows are populated).
CREATE INDEX IF NOT EXISTS idx_interventions_fired_response_id
    ON steering.interventions (fired_response_id)
    WHERE fired_response_id IS NOT NULL;

-- ─── 4. Index auto_enrol for the bulk INSERT…SELECT in enrol_auto_steerers ──
-- The enrol_auto_steerers query filters on auto_enrol AND enabled AND
-- deleted_at IS NULL. A partial index on (auto_enrol=true, enabled=true,
-- deleted_at IS NULL) gives a near-zero-cost lookup for the handful of
-- auto-enrolling steerers (typically 1–3 rows on the current registry).
CREATE INDEX IF NOT EXISTS idx_steerers_auto_enrol_active
    ON steering.steerers (id, default_mode_id, authority_id)
    WHERE auto_enrol = true
      AND enabled    = true
      AND deleted_at IS NULL;
