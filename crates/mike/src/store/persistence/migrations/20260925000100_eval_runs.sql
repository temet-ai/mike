--
-- eval.runs, eval.run_items and eval.gates: the offline judge harness in
-- mike (`mike eval run` and `mike eval gate`), replacing the uncommitted
-- tools/judge-harness and the fleet-mirror W4 replay workflow.
--
-- Why. On 2026-08-31 the turn judge's model, rubric and input changed within
-- 17 minutes with no before/after measurement, and stored `corrected`
-- recall on real operator turns fell from 24.8% to 6.8% unnoticed for three
-- weeks (fleet-mirror analysis.md). Nothing in the DB recorded what any
-- judge configuration scored against labelled data. These tables do.
--
-- eval.runs: one row per `mike eval run`. A run judges a fixed label set
-- (split, tiers, holdout batch, labels_as_of) with one configuration (a
-- bundle plus the rubric, input builder, slices and model it resolves to).
-- It is mutable only in its status, cost and report columns, because a run
-- can pause on the spend ceiling and resume later (`--resume`). label_ids_hash
-- is data, compared in code to prove two runs judged the same items; it is
-- not indexed and never used to look a run up (sql-set-based.md).
--
-- eval.run_items: one row per judged subject turn in a run, linking the
-- run, the gold labels it was scored against and the memory.turn_verdicts
-- row (mode = 'eval') that holds the verdict and its exact input. A refused
-- or failed call leaves turn_verdict_id NULL with outcome refused/failed, so
-- an incomplete run is visible and a gate over it fails closed.
--
-- eval.gates: one row per `mike eval gate` decision: candidate run against
-- baseline run on the same items, optionally with a noise run (a repeat that
-- measures how much a configuration disagrees with itself). The verdict and
-- the full report are kept whether it passed or not, so the number of gates
-- spent on one holdout batch can be counted.

CREATE SCHEMA IF NOT EXISTS eval;

CREATE TABLE IF NOT EXISTS eval.runs (
    id                     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    created_at             timestamptz NOT NULL DEFAULT now(),
    finished_at            timestamptz,
    purpose                text NOT NULL
                             CHECK (purpose IN ('measure', 'noise', 'ablation')),
    status                 text NOT NULL DEFAULT 'running'
                             CHECK (status IN ('running', 'paused_budget', 'complete',
                                               'failed', 'cancelled')),
    bundle_id              bigint NOT NULL REFERENCES eval.bundles(id),
    rubric_version         text NOT NULL,
    input_builder_version  text NOT NULL,
    slices                 text[] NOT NULL DEFAULT '{}',
    model                  text NOT NULL,
    split                  text NOT NULL CHECK (split IN ('dev', 'holdout')),
    tiers                  text[] NOT NULL,
    holdout_batch          text,
    labels_as_of           timestamptz NOT NULL,
    label_count            integer NOT NULL CHECK (label_count >= 0),
    label_ids_hash         text NOT NULL,
    repeat_of_run_id       bigint REFERENCES eval.runs(id),
    max_usd                double precision NOT NULL CHECK (max_usd >= 0),
    cost_usd               double precision NOT NULL DEFAULT 0,
    items_judged           integer NOT NULL DEFAULT 0,
    items_refused          integer NOT NULL DEFAULT 0,
    items_failed           integer NOT NULL DEFAULT 0,
    report                 jsonb,
    argv                   jsonb NOT NULL,
    CONSTRAINT eval_runs_holdout_batch_check
        CHECK (split = 'dev' OR holdout_batch IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS idx_eval_runs_created
    ON eval.runs (created_at DESC);

CREATE INDEX IF NOT EXISTS idx_eval_runs_bundle
    ON eval.runs (bundle_id);

CREATE INDEX IF NOT EXISTS idx_eval_runs_repeat_of
    ON eval.runs (repeat_of_run_id) WHERE repeat_of_run_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS eval.run_items (
    run_id                  bigint NOT NULL REFERENCES eval.runs(id) ON DELETE CASCADE,
    operator_turn_id        bigint NOT NULL
                              REFERENCES memory.operator_turns(id) ON DELETE CASCADE,
    reaction_label_id       bigint REFERENCES eval.labels(id),
    delivery_label_id       bigint REFERENCES eval.labels(id),
    turn_verdict_id         bigint REFERENCES memory.turn_verdicts(id) ON DELETE SET NULL,
    outcome                 text NOT NULL
                              CHECK (outcome IN ('judged', 'refused', 'failed',
                                                 'skipped_unresolved')),
    stratum                 text NOT NULL DEFAULT 'all',
    inclusion_prob          double precision
                              CHECK (inclusion_prob IS NULL
                                     OR (inclusion_prob > 0 AND inclusion_prob <= 1)),
    reaction_correct        boolean,
    delivery_correct        boolean,
    created_at              timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, operator_turn_id),
    CONSTRAINT eval_run_items_judged_check
        CHECK ((outcome = 'judged') = (turn_verdict_id IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_eval_run_items_turn
    ON eval.run_items (operator_turn_id);

CREATE INDEX IF NOT EXISTS idx_eval_run_items_verdict
    ON eval.run_items (turn_verdict_id) WHERE turn_verdict_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_eval_run_items_reaction_label
    ON eval.run_items (reaction_label_id) WHERE reaction_label_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_eval_run_items_delivery_label
    ON eval.run_items (delivery_label_id) WHERE delivery_label_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS eval.gates (
    id                  bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    created_at          timestamptz NOT NULL DEFAULT now(),
    candidate_run_id    bigint NOT NULL REFERENCES eval.runs(id),
    baseline_run_id     bigint NOT NULL REFERENCES eval.runs(id),
    noise_run_id        bigint REFERENCES eval.runs(id),
    holdout_batch       text,
    verdict             text NOT NULL CHECK (verdict IN ('pass', 'fail', 'incomplete')),
    floors              jsonb NOT NULL,
    report              jsonb NOT NULL,
    override_reason     text,
    CONSTRAINT eval_gates_distinct_runs_check CHECK (candidate_run_id <> baseline_run_id)
);

CREATE INDEX IF NOT EXISTS idx_eval_gates_candidate ON eval.gates (candidate_run_id);
CREATE INDEX IF NOT EXISTS idx_eval_gates_baseline ON eval.gates (baseline_run_id);
CREATE INDEX IF NOT EXISTS idx_eval_gates_noise
    ON eval.gates (noise_run_id) WHERE noise_run_id IS NOT NULL;

-- The holdout-reuse guard counts gates per holdout batch.
CREATE INDEX IF NOT EXISTS idx_eval_gates_holdout_batch
    ON eval.gates (holdout_batch, created_at) WHERE holdout_batch IS NOT NULL;

-- eval.bundles (JC06) gains the shadow judge's role and a foreign key from its
-- gate column to the gate decision. The role is minted by JC06's resolver from the
-- judge_shadow.* settings, like every other resolver role; JC06's CHECK is widened
-- here, as its spec anticipates. JC06 names the gate column gate_run_id; the pass or
-- fail decision is a row of eval.gates, so that is what it references.
ALTER TABLE eval.bundles
    DROP CONSTRAINT IF EXISTS bundles_role_check;

ALTER TABLE eval.bundles
    ADD CONSTRAINT bundles_role_check
    CHECK (role IN ('turn_judge', 'session_judge', 'correction_classifier',
                    'turn_fast_path', 'steerer', 'turn_judge_shadow'));

ALTER TABLE eval.bundles
    DROP CONSTRAINT IF EXISTS bundles_gate_run_id_fkey;

ALTER TABLE eval.bundles
    ADD CONSTRAINT bundles_gate_run_id_fkey
    FOREIGN KEY (gate_run_id) REFERENCES eval.gates(id);

CREATE INDEX IF NOT EXISTS idx_eval_bundles_gate_run_id
    ON eval.bundles (gate_run_id) WHERE gate_run_id IS NOT NULL;

COMMENT ON TABLE eval.runs IS
    'One row per mike eval run: a fixed label set judged by one '
    'configuration. Mutable only in status, finished_at, cost_usd, item '
    'counters and report (a run can pause on the spend ceiling and resume).';

COMMENT ON TABLE eval.run_items IS
    'One row per subject turn in an eval run: its gold label ids, the '
    'memory.turn_verdicts row (mode eval) holding the verdict and exact '
    'input, and whether each axis matched the gold label.';

COMMENT ON TABLE eval.gates IS
    'One row per mike eval gate decision (candidate vs baseline run on the '
    'same items). Kept whether it passed or not, so holdout reuse is '
    'countable.';

COMMENT ON COLUMN eval.runs.label_ids_hash IS
    'sha256 over the sorted label ids the run judged. Data compared in '
    'code to prove two runs share items; never indexed, never a lookup key.';

COMMENT ON COLUMN eval.run_items.inclusion_prob IS
    'Copied from the item''s label (eval.labels.inclusion_prob). NULL when '
    'the label has none, as for the 78 fleet-mirror ground-truth turns outside '
    'the real-turn scope; such items count in unweighted scores and are '
    'excluded from every Horvitz-Thompson weighted figure, never weighted as 1.';
