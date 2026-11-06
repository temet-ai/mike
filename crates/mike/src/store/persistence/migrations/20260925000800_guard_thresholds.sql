-- Calibrated confidence thresholds for the turn-end guard's questions
-- (plans/judgement-context/wp/JC18-cross-session-calibration-directives.md,
-- part A3). The guard's static threshold is one number per steerer row
-- (SteererParams.confidence_threshold, default 0.7). JC18 derives a
-- threshold per question and per scope (a project, or the whole install)
-- from the candidate confidence bins JC18's own family (job J2b) writes to
-- eval.rates in JC08's daily rollup (family 'steer_calibration', axis
-- 'steer_candidate', dimension 'confidence_bin_05'; candidates are gate-open
-- evaluations with applies = true, cleared or not), and records
-- every computation here -- applied or not -- so that:
--   * the threshold in force at any past moment can be rebuilt (the as-of
--     rule every context builder in this program obeys);
--   * a change of guard behaviour is never unseen: every 'applied' row gets a
--     config.change_log row (subject_kind 'guard_threshold', via 'calibration',
--     detail.threshold_row_id = this id) written in the same statement, and the
--     guard bundle JC06's resolver mints afterwards lists this id in
--     components.calibration.thresholds_in_force. The bundle is minted after the
--     row, so the row carries no bundle id; the link runs from the bundle.
--
-- Append-only. One row per (steerer, question, scope, project) per
-- eval.rollups generation (rollup_id). Nothing updates or deletes a
-- row; the row in force for a key is the latest with decision = 'applied',
-- or the static threshold when there is none.
--
-- steerer_id, project_id and rollup_id are plain bigints with NO foreign key,
-- for the reason 20260829000002_llm_calls.sql gives for its own ids: a
-- recording write must never fail because of the order two writers ran in.
-- An FK would also either block deleting the guard steerer once history exists
-- (RESTRICT) or silently delete threshold history with a project (CASCADE),
-- which breaks the as-of rebuild.
--
-- threshold_floor is the question's floor from the guard row's
-- config.questions.<question>.floor (JC16); threshold_ceiling is the
-- guard.threshold.ceiling setting; precision_bar is JC17's bars.rs constant.
--
-- question mirrors the guard's question slugs (JC17 contract C2, the same
-- CHECK JC14 puts on steering.evaluations.question). A fifth question extends
-- this CHECK in the migration that adds it. scope uses JC08's vocabulary
-- (eval.rates.scope_kind): 'project' or 'global'.

CREATE TABLE IF NOT EXISTS steering.guard_thresholds (
    id                   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    steerer_id           bigint NOT NULL,
    question             text NOT NULL
                           CHECK (question IN ('relapse', 'unbacked_wait', 'authorised_handback', 'unobserved_done')),
    scope                text NOT NULL CHECK (scope IN ('project', 'global')),
    project_id           bigint,
    computed_at          timestamptz NOT NULL DEFAULT now(),
    rollup_id            bigint NOT NULL,
    rollup_finished_at   timestamptz NOT NULL,
    window_start         timestamptz NOT NULL,
    window_end           timestamptz NOT NULL,
    label_basis          text NOT NULL
                           CHECK (label_basis IN ('auto_shadow_gold_relayed', 'holdback_gold_relayed')),
    question_mode        text NOT NULL CHECK (question_mode IN ('off', 'shadow', 'append', 'wake')),
    threshold_mode       text NOT NULL CHECK (threshold_mode IN ('off', 'shadow', 'apply')),
    n_labelled           integer NOT NULL CHECK (n_labelled >= 0),
    precision_bar        double precision NOT NULL CHECK (precision_bar > 0 AND precision_bar <= 1),
    threshold_floor      real NOT NULL,
    threshold_ceiling    real NOT NULL,
    static_threshold     real NOT NULL,
    candidate_threshold  real,
    applied_threshold    real NOT NULL,
    decision             text NOT NULL
                           CHECK (decision IN ('applied', 'shadow_only', 'held_hysteresis', 'held_cooldown',
                                               'held_wake_floor', 'insufficient', 'mode_off')),
    detail               jsonb NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT guard_thresholds_scope_project CHECK ((scope = 'project') = (project_id IS NOT NULL)),
    CONSTRAINT guard_thresholds_window CHECK (window_start < window_end),
    CONSTRAINT guard_thresholds_bounds CHECK (threshold_floor <= threshold_ceiling),
    CONSTRAINT guard_thresholds_candidate_in_bounds
        CHECK (candidate_threshold IS NULL
               OR (candidate_threshold >= threshold_floor AND candidate_threshold <= threshold_ceiling)),
    CONSTRAINT guard_thresholds_candidate_when_decided
        CHECK (decision IN ('insufficient', 'mode_off') OR candidate_threshold IS NOT NULL)
);

COMMENT ON TABLE steering.guard_thresholds IS
    'Append-only history of calibrated guard thresholds (JC18). One row per (steerer, question, scope, project) per eval.rollups generation. The threshold in force for a key is the latest row with decision = ''applied''; with none, the steerer row''s static threshold applies. detail holds every confidence bin used and the Wilson bounds, in full.';
COMMENT ON COLUMN steering.guard_thresholds.rollup_id IS
    'The eval.rollups generation these numbers came from (JC08). Rebuilding the guard''s input as of time T uses the newest rollup with finished_at <= T.';
COMMENT ON COLUMN steering.guard_thresholds.label_basis IS
    'Which labels counted. A question in shadow: auto_shadow, gold and relayed. A question in append or wake: gold, relayed and the next-message labels of arm = ''holdback'' evaluations (JC17), never the automatic label of a delivered fire (a nudge that works prevents the correction it is scored by).';
COMMENT ON COLUMN steering.guard_thresholds.applied_threshold IS
    'The threshold in force for this key after this row. Equals the previous applied value unless decision = ''applied''.';
COMMENT ON COLUMN steering.guard_thresholds.detail IS
    'Every confidence bin used (k, n, lower edge), the Wilson bounds, and previous_threshold (the value in force before this row). Full, never cut.';

-- Serves both the snapshot load (latest row per key, DISTINCT ON over
-- steerer_id, question, scope, project_id ORDER BY computed_at DESC) and the
-- per-key history the SPA pages through.
CREATE INDEX IF NOT EXISTS idx_guard_thresholds_key_latest
    ON steering.guard_thresholds (steerer_id, question, scope, project_id, computed_at DESC);

-- Serves the as-of rebuild and the "applied changes in the last N days"
-- count on the calibration panel.
CREATE INDEX IF NOT EXISTS idx_guard_thresholds_applied
    ON steering.guard_thresholds (computed_at DESC)
    WHERE decision = 'applied';
