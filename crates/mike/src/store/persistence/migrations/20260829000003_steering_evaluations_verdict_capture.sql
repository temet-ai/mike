-- artifacts/2026-08-29/llm-cost-and-signal-plan.md, "Keeping the passes".
--
-- steering.evaluations (20260805400001_steering_evaluations.sql) already
-- records every verifier evaluation attempt, but a `resolved_pass` row
-- carries nothing beyond the status word — the model's own reasoning for
-- NOT firing is discarded, so "why did the verifier wave this through" is
-- unanswerable after the fact. These three columns let the write path
-- (crates/mike/src/steerer/) retain the full verdict on every evaluation,
-- gated by the `steerer.retain_pass_verdicts` setting, not only on a fire
-- (steering.interventions already covers fires).
--
-- All three are nullable with no default: a `dispatched` row never has
-- them (the verdict does not exist yet), and a `resolved_correction` row
-- may leave them NULL when `steerer.retain_pass_verdicts` is off — the
-- column is additive, not a new invariant on the table.

ALTER TABLE steering.evaluations
    ADD COLUMN IF NOT EXISTS verdict_json jsonb,
    ADD COLUMN IF NOT EXISTS confidence   double precision,
    ADD COLUMN IF NOT EXISTS reason       text;

COMMENT ON COLUMN steering.evaluations.verdict_json IS
    'Full structured verdict the verifier LLM returned for this evaluation '
    '(schema owned by crates/mike/src/steerer/), genuinely schemaless from '
    'this table''s point of view. NULL until resolved, and NULL when '
    'steerer.retain_pass_verdicts is off.';

COMMENT ON COLUMN steering.evaluations.confidence IS
    'Confidence in [0.0, 1.0] the verifier attached to this verdict. NULL '
    'until resolved.';

COMMENT ON COLUMN steering.evaluations.reason IS
    'Free-text reason the verifier gave for its verdict — the model''s own '
    'explanation for why it did or did not fire. NULL until resolved.';
