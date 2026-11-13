
-- Judgement metrics and drift alarms.
--
-- On 2026-08-31 the turn judge changed model (kimi-k3 to gemini-3-flash-preview),
-- rubric (r1 to r3) and input in one step. Stored `corrected` on real operator
-- turns fell from 19.6% (336 of 1,715) to 3.4% (32 of 929) and nobody noticed for
-- three weeks. The judge also stopped writing verdicts four times between
-- 2026-07-30 and 2026-09-21 and was refused 64-91% of its calls on twelve days,
-- with nothing recording any of it. This migration adds what a daily SQL-only
-- rollup and a 15-minute alarm pass write to. Nothing here calls an LLM, sends
-- captured text anywhere, or changes Claude traffic.

CREATE SCHEMA IF NOT EXISTS eval;

-- One row per rolled-up day and SQL version. The UNIQUE constraint is the
-- exactly-once claim: the worker inserts with ON CONFLICT DO NOTHING RETURNING id
-- as the first statement of the rollup transaction, so a restart or a second
-- instance cannot write the same day twice. A new sql_version may re-roll a day
-- (after a fix to a family query); readers take the newest rollup per day.
-- finished_at is written by the same transaction, so a committed row always has
-- it; the column is nullable only because the claim precedes the family inserts.
CREATE TABLE IF NOT EXISTS eval.rollups (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    for_day       date NOT NULL,
    sql_version   text NOT NULL,
    mode          text NOT NULL CHECK (mode IN ('scheduled', 'backfill', 'manual')),
    labels_as_of  timestamptz NOT NULL,
    started_at    timestamptz NOT NULL DEFAULT now(),
    finished_at   timestamptz,
    rows_written  integer,
    CONSTRAINT uq_eval_rollups_day_version UNIQUE (for_day, sql_version)
);

COMMENT ON TABLE eval.rollups IS
    'One committed daily metrics rollup (JC08). Windows end at for_day 00:00 UTC.';

-- Append-only. Every row is k of n over a stated population, window and label
-- slice, so any value can be recomputed by hand from the source rows.
-- bundle_id, project_id and steerer_id are plain bigint with NO foreign key, for
-- the reason 20260829000002_llm_calls.sql gives: a recording write must never
-- fail because a referenced row moved. bundle_label keeps verdicts written before
-- eval.bundles existed attributable: 'legacy:<judge_model>:<rubric_version>'.
-- bundle_label NULL on an accuracy or label_mix row means every writer of the
-- role combined (what readers of outcome_id actually consume).
-- family, metric, population and scope_kind are closed sets; a later package
-- that adds a family extends these CHECKs in its own migration.
CREATE TABLE IF NOT EXISTS eval.rates (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    rollup_id        bigint NOT NULL REFERENCES eval.rollups(id) ON DELETE CASCADE,
    family           text NOT NULL
                       CHECK (family IN ('accuracy', 'label_mix', 'base_rate', 'steer_calibration',
                                         'pattern_tagging')),
    metric           text NOT NULL
                       CHECK (metric IN ('precision', 'recall', 'share', 'fire_rate', 'fire_precision',
                                         'fire_recall')),
    population       text NOT NULL
                       CHECK (population IN ('real_turns', 'replied_real_turns', 'real_session_ends',
                                             'steerer_evaluations', 'labelled_turns')),
    scope_kind       text NOT NULL CHECK (scope_kind IN ('global', 'project')),
    project_id       bigint,
    window_start     timestamptz,
    window_end       timestamptz NOT NULL,
    window_days      integer CHECK (window_days IS NULL OR window_days > 0),
    role             text NOT NULL
                       CHECK (role IN ('turn_judge', 'session_judge', 'steerer')),
    bundle_id        bigint,
    bundle_label     text,
    axis             text NOT NULL,
    class            text NOT NULL,
    label_tier       text,
    label_split      text,
    label_source     text,
    dimension        text,
    dimension_value  text,
    steerer_id       bigint,
    question         text,
    mode             text,
    k                integer NOT NULL CHECK (k >= 0),
    n                integer NOT NULL CHECK (n >= k),
    value            double precision,
    ci_low           double precision,
    ci_high          double precision,
    weighted         boolean NOT NULL DEFAULT false,
    n_effective      double precision,
    detail           jsonb NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT eval_rates_family_metric CHECK (
        (family = 'accuracy'          AND metric IN ('precision', 'recall')) OR
        (family IN ('label_mix', 'base_rate') AND metric = 'share') OR
        (family = 'steer_calibration' AND metric IN ('fire_rate', 'fire_precision', 'fire_recall'))),
    CONSTRAINT eval_rates_scope_project CHECK ((scope_kind = 'global') = (project_id IS NULL)),
    CONSTRAINT eval_rates_window_pair CHECK ((window_start IS NULL) = (window_days IS NULL))
);

COMMENT ON COLUMN eval.rates.value IS
    'k/n, or the Horvitz-Thompson weighted estimate when weighted is true (weights 1/inclusion_prob from eval.labels).';
COMMENT ON COLUMN eval.rates.ci_low IS
    'Wilson 95% interval; for weighted rows it is computed on n_effective (Kish).';

CREATE INDEX IF NOT EXISTS idx_eval_rates_rollup_family
    ON eval.rates (rollup_id, family, role);

CREATE INDEX IF NOT EXISTS idx_eval_rates_trend
    ON eval.rates (family, role, axis, class, rollup_id);

-- Drift and liveness alarms. One open episode per (kind, subject): the partial
-- unique index is the conflict target of the alarm upsert
-- (ON CONFLICT (kind, subject) WHERE cleared_at IS NULL DO UPDATE), the same
-- raise-once idea as agents.work_item_attention (20260923200000) but with an
-- episode that closes: cleared_at is set after three consecutive evaluations in
-- which the alarm did not fire, and a later firing opens a new row.
-- Acknowledgement is the operator's (SPA only); it does not clear the episode, it
-- only stops scripts/usage-audit.sh from exiting 1 on it.
-- kind is closed: embedding_drift (JC20), metric_drop and shadow_disagreement
-- (JC12) are reserved, and each is added by its own package's migration.
-- gold_label_by_agent (JC08 section 4.4.5a) and the family pattern_tagging (JC20's, reserved
-- here so JC20 re-creates no family CHECK) were added on 2026-09-24.
CREATE TABLE IF NOT EXISTS eval.alarms (
    id                 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind               text NOT NULL
                         CHECK (kind IN ('label_mix_shift', 'unjudged_backlog', 'judge_stalled',
                                         'budget_refusals', 'change_without_gate',
                                         'gold_label_by_agent')),
    subject            text NOT NULL,
    first_seen_at      timestamptz NOT NULL DEFAULT now(),
    last_seen_at       timestamptz NOT NULL DEFAULT now(),
    cleared_at         timestamptz,
    evaluations        integer NOT NULL DEFAULT 1 CHECK (evaluations >= 1),
    quiet_evaluations  integer NOT NULL DEFAULT 0 CHECK (quiet_evaluations >= 0),
    detail             jsonb NOT NULL,
    acknowledged_at    timestamptz,
    ack_note           text
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_eval_alarms_open
    ON eval.alarms (kind, subject) WHERE cleared_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_eval_alarms_first_seen
    ON eval.alarms (first_seen_at DESC);

-- Wilson 95% score interval, used by every family so intervals are computed one
-- way. n is a count, or Kish's effective sample size for weighted rows. Returns
-- NULL when there is nothing to bound.
CREATE OR REPLACE FUNCTION eval.wilson_low(p double precision, n double precision)
RETURNS double precision
LANGUAGE sql IMMUTABLE PARALLEL SAFE
AS $$
    SELECT CASE WHEN p IS NULL OR n IS NULL OR n <= 0 THEN NULL
                ELSE greatest(0.0,
                     (p + 3.8414588 / (2 * n)
                        - 1.959964 * sqrt(p * (1 - p) / n + 3.8414588 / (4 * n * n)))
                     / (1 + 3.8414588 / n))
           END
$$;

CREATE OR REPLACE FUNCTION eval.wilson_high(p double precision, n double precision)
RETURNS double precision
LANGUAGE sql IMMUTABLE PARALLEL SAFE
AS $$
    SELECT CASE WHEN p IS NULL OR n IS NULL OR n <= 0 THEN NULL
                ELSE least(1.0,
                     (p + 3.8414588 / (2 * n)
                        + 1.959964 * sqrt(p * (1 - p) / n + 3.8414588 / (4 * n * n)))
                     / (1 + 3.8414588 / n))
           END
$$;

-- Which production class is scored against which label value. Data, not code:
-- JC09 adds the reaction and delivery rows, JC13 the session_end rows, each in
-- its own migration. replied_counts_as_negative marks a class whose label only
-- exists on session-ending turns: a turn the operator replied to cannot have
-- ended its session, so it counts as a labelled negative (this is how the
-- fleet-mirror run got "abandoned matched a frustration close on 27 of 626").
CREATE TABLE IF NOT EXISTS eval.class_map (
    production_axis             text NOT NULL,
    production_class            text NOT NULL,
    label_axis                  text NOT NULL,
    label_value                 text NOT NULL,
    replied_counts_as_negative  boolean NOT NULL DEFAULT false,
    PRIMARY KEY (production_axis, production_class)
);

INSERT INTO eval.class_map
    (production_axis, production_class, label_axis, label_value, replied_counts_as_negative)
VALUES
    ('legacy_outcome',         'corrected', 'reaction',    'corrected',             false),
    ('legacy_outcome',         'abandoned', 'session_end', 'closed_in_frustration', true),
    ('legacy_session_outcome', 'failure',   'session_end', 'closed_in_frustration', false)
ON CONFLICT (production_axis, production_class) DO NOTHING;

-- The one place the metrics read the turn population from. Column names on
-- memory.turn_facts are fixed by the JC02 spec (session_class, prompt_kind,
-- next_genuine_request_id, next_state, ended_on, close_cause); memory.operator_turns
-- .outcome_bundle_id and eval.bundles are JC06's. writer_role names the writer of
-- the stored outcome. The live writers are the judge and the abort path (JC00b):
-- the abort path stamps no judge model and no bundle, so it is recognised by
-- JC02's close_cause = 'abort' on an 'abandoned' turn. The correction classifier
-- and the keyword fast path are not live (the classifier is off in boot config,
-- the fast path cannot fire); a JC06 bundle role still names them should either
-- write again. Anything else judge-less is 'unattributed': on history, the 57
-- classifier labels written before the classifier stopped being wired.
-- Added 2026-09-24: is_real also excludes a turn whose start request carries
-- claude_code_agent_id (binding correction 9; a no-op once JC01's backfill has run)
-- and any session or turn in eval.population_exclusions (JC07's import: the
-- 2026-07-27 skill-eval batch, scratch sessions and probe prompts). The NOT EXISTS is an
-- anti-join on the exclusion key, which sql-set-based.md allows.
CREATE OR REPLACE VIEW eval.turn_population_v AS
SELECT t.id                                   AS operator_turn_id,
       t.session_id,
       t.project_id,
       t.created_at,
       t.closed_at,
       t.classified_at,
       o.code                                 AS legacy_outcome,
       t.outcome_bundle_id                    AS bundle_id,
       CASE WHEN t.outcome_bundle_id IS NOT NULL THEN 'bundle:' || t.outcome_bundle_id::text
            ELSE 'legacy:' || coalesce(t.judge_model, 'none') || ':'
                           || coalesce(t.judge_rubric_version, 'none')
       END                                    AS bundle_label,
       CASE WHEN b.role IS NOT NULL THEN b.role
            WHEN t.judge_model IS NOT NULL THEN 'turn_judge'
            WHEN o.code = 'unknown' THEN NULL
            WHEN o.code = 'abandoned' AND f.close_cause = 'abort' THEN 'abort_path'
            ELSE 'unattributed'
       END                                    AS writer_role,
       coalesce(f.session_class = 'operator' AND f.prompt_kind = 'genuine', false)
         AND sr.claude_code_agent_id IS NULL
         AND NOT EXISTS (SELECT 1 FROM eval.population_exclusions x
                          WHERE x.session_id = t.session_id
                            AND x.withdrawn_at IS NULL
                            AND (x.operator_turn_id IS NULL OR x.operator_turn_id = t.id)) AS is_real,
       (f.next_genuine_request_id IS NOT NULL) AS is_replied,
       f.next_state,
       f.ended_on,
       f.close_cause
  FROM memory.operator_turns t
  JOIN memory.turn_outcomes o ON o.id = t.outcome_id
  LEFT JOIN eval.bundles b ON b.id = t.outcome_bundle_id
  LEFT JOIN memory.turn_facts f ON f.operator_turn_id = t.id
  LEFT JOIN requests.requests sr ON sr.id = t.start_request_id
 WHERE t.closed_at IS NOT NULL;

COMMENT ON VIEW eval.turn_population_v IS
    'Closed turns with the JC02 real/replied/settled flags, the label''s bundle and writer role; read by the JC08 rollup and alarms.';
