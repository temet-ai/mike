-- JC13 part b: why a session ended, on session_judges.verdicts.
--
-- Why. session_judges.verdicts records how a session went (success, partial, failure, unknown) but
-- never why it ended, and mike's only end signal, the turn label `abandoned`, matched a real
-- frustration close on 27 of 626 real turns (fleet-mirror session-ends.md). JC13 adds the end label
-- (done, closed_in_frustration, cut_off_neutral, fleet_seat, unclear) with the labelling rubric's
-- evidence, failure, failure tags and severity, says who decided it (the model or a deterministic
-- rule), and adds `mode` so that shadow and eval verdicts can share the table without any existing
-- reader seeing them. Every existing row is a live verdict with no end label.
--
-- Expand-only: nullable or defaulted columns, CHECKs that hold on existing rows, new indexes, views
-- re-created with the same leading columns, one new table, widened CHECK lists, and one NOT NULL
-- relaxed on a JC10 table. A revert is a binary rollback (scripts/deploy-proxy.sh:25-36).

SET LOCAL lock_timeout = '5s';

-- ── 1. End-label and provenance columns on session_judges.verdicts ─────────────────────────────

ALTER TABLE session_judges.verdicts
    ADD COLUMN IF NOT EXISTS mode              text             NOT NULL DEFAULT 'live',
    ADD COLUMN IF NOT EXISTS bundle_id         bigint           NULL,
    ADD COLUMN IF NOT EXISTS rubric_id         text             NULL,
    ADD COLUMN IF NOT EXISTS end_turn_id       bigint           NULL,
    ADD COLUMN IF NOT EXISTS end_label         text             NULL,
    ADD COLUMN IF NOT EXISTS end_source        text             NULL,
    ADD COLUMN IF NOT EXISTS end_rule          text             NULL,
    ADD COLUMN IF NOT EXISTS end_confidence    text             NULL,
    ADD COLUMN IF NOT EXISTS end_evidence      text             NULL,
    ADD COLUMN IF NOT EXISTS failure           text             NULL,
    ADD COLUMN IF NOT EXISTS failure_tags      text[]           NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS severity          text             NULL,
    ADD COLUMN IF NOT EXISTS slices            text[]           NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS as_of             timestamptz      NULL,
    ADD COLUMN IF NOT EXISTS digest_version    text             NULL,
    ADD COLUMN IF NOT EXISTS input_text        text             NULL,
    ADD COLUMN IF NOT EXISTS llm_output        jsonb            NULL,
    ADD COLUMN IF NOT EXISTS generation_id     text             NULL,
    ADD COLUMN IF NOT EXISTS cost_usd          double precision NULL,
    ADD COLUMN IF NOT EXISTS prompt_tokens     integer          NULL,
    ADD COLUMN IF NOT EXISTS completion_tokens integer          NULL,
    ADD COLUMN IF NOT EXISTS latency_ms        integer          NULL;

ALTER TABLE session_judges.verdicts DROP CONSTRAINT IF EXISTS verdicts_mode_check;
ALTER TABLE session_judges.verdicts ADD CONSTRAINT verdicts_mode_check
    CHECK (mode IN ('live', 'shadow', 'eval'));

ALTER TABLE session_judges.verdicts DROP CONSTRAINT IF EXISTS verdicts_mode_bundle_check;
ALTER TABLE session_judges.verdicts ADD CONSTRAINT verdicts_mode_bundle_check
    CHECK (mode = 'live' OR bundle_id IS NOT NULL);

ALTER TABLE session_judges.verdicts DROP CONSTRAINT IF EXISTS verdicts_end_label_check;
ALTER TABLE session_judges.verdicts ADD CONSTRAINT verdicts_end_label_check
    CHECK (end_label IS NULL OR end_label IN
           ('done', 'closed_in_frustration', 'cut_off_neutral', 'fleet_seat', 'unclear'));

ALTER TABLE session_judges.verdicts DROP CONSTRAINT IF EXISTS verdicts_end_source_check;
ALTER TABLE session_judges.verdicts ADD CONSTRAINT verdicts_end_source_check
    CHECK (end_source IS NULL OR end_source IN ('llm', 'deterministic'));

ALTER TABLE session_judges.verdicts DROP CONSTRAINT IF EXISTS verdicts_end_rule_check;
ALTER TABLE session_judges.verdicts ADD CONSTRAINT verdicts_end_rule_check
    CHECK ((end_rule IS NULL OR end_rule IN ('fleet-seat-v1', 'done-v1'))
           AND (end_rule IS NULL OR end_source = 'deterministic')
           AND (end_source IS DISTINCT FROM 'deterministic' OR end_rule IS NOT NULL));

ALTER TABLE session_judges.verdicts DROP CONSTRAINT IF EXISTS verdicts_end_confidence_check;
ALTER TABLE session_judges.verdicts ADD CONSTRAINT verdicts_end_confidence_check
    CHECK (end_confidence IS NULL OR end_confidence IN ('high', 'medium', 'low'));

ALTER TABLE session_judges.verdicts DROP CONSTRAINT IF EXISTS verdicts_severity_check;
ALTER TABLE session_judges.verdicts ADD CONSTRAINT verdicts_severity_check
    CHECK (severity IS NULL OR severity IN ('none', 'minor', 'major', 'severe'));

ALTER TABLE session_judges.verdicts DROP CONSTRAINT IF EXISTS verdicts_end_shape_check;
ALTER TABLE session_judges.verdicts ADD CONSTRAINT verdicts_end_shape_check
    CHECK (end_label IS NULL
           OR (end_turn_id IS NOT NULL AND end_source IS NOT NULL
               AND end_confidence IS NOT NULL AND severity IS NOT NULL));

-- The latest live verdict per session: every existing reader (JC13 §3.2 R1-R11) and
-- mike.session_quality_v now filter mode = 'live'.
CREATE INDEX IF NOT EXISTS idx_sj_verdicts_session_mode_judged
    ON session_judges.verdicts (session_id, mode, judged_at DESC, id DESC);

-- The projection branch, the read functions and the v3 selection's anti-join look up the live
-- end label of an ending turn.
CREATE INDEX IF NOT EXISTS idx_sj_verdicts_live_end_turn
    ON session_judges.verdicts (end_turn_id)
    WHERE mode = 'live' AND end_turn_id IS NOT NULL;

-- Shadow and eval rows of one bundle, and JC08's accuracy family by bundle.
CREATE INDEX IF NOT EXISTS idx_sj_verdicts_bundle_mode_judged
    ON session_judges.verdicts (bundle_id, mode, judged_at);

COMMENT ON COLUMN session_judges.verdicts.mode IS
    'live: the production session judge (every row before JC13). shadow: a candidate bundle on '
    'settled sessions, never read by the existing session readers. eval: a mike eval run item. '
    'Readers of "the session''s verdict" read live rows only.';
COMMENT ON COLUMN session_judges.verdicts.end_turn_id IS
    'The ending turn (the session''s last genuine operator turn, settled) this row''s end label '
    'applies to. No FK: recording must not fail and retention may delete the turn. The label stops '
    'applying when that turn later gets a reply (memory.turn_facts.next_state becomes resolved).';
COMMENT ON COLUMN session_judges.verdicts.end_label IS
    'Why the session ended, per session-end-rubric.md: done, closed_in_frustration, '
    'cut_off_neutral, fleet_seat or unclear. NULL on rows before JC13 and when the session had no '
    'settled genuine ending turn. An operator gold label in eval.labels wins over it at read time.';
COMMENT ON COLUMN session_judges.verdicts.end_source IS
    'llm: the model decided end_label. deterministic: end_rule decided it and the model''s own '
    'answer is kept in llm_output.';
COMMENT ON COLUMN session_judges.verdicts.failure_tags IS
    'Up to three tags from the open failure vocabulary of labelling-rubric.md; a tag outside it '
    'is stored with a new: prefix. Not CHECKed: the vocabulary is open.';
COMMENT ON COLUMN session_judges.verdicts.input_text IS
    'The exact user message sent to the model, whole (inspection-fidelity Layer 1). NULL on rows '
    'written before JC13.';
COMMENT ON COLUMN session_judges.verdicts.llm_output IS
    'The parsed structured output, whole, including the model''s end label when a deterministic '
    'rule overrode it.';
COMMENT ON COLUMN session_judges.verdicts.generation_id IS
    'OpenRouter response id. A fingerprint for cross-reference, never a lookup key: deliberately '
    'not indexed.';

-- ── 2. mike.session_quality_v reads live rows only and carries the end fields ──────────────────
--
-- The eight existing columns (JC06 appended bundle_id) keep their names and order
-- (mike.session_summary_v depends on the view); ten columns are appended.

CREATE OR REPLACE VIEW mike.session_quality_v AS
SELECT DISTINCT ON (v.session_id)
    s.session_uuid::text AS session_id,
    o.code               AS outcome,
    v.quality_score,
    v.rationale,
    v.judge_model,
    v.rubric_version,
    v.judged_at,
    v.bundle_id,
    v.id                 AS verdict_id,
    v.end_turn_id,
    v.end_label,
    v.end_source,
    v.end_rule,
    v.end_confidence,
    v.end_evidence,
    v.failure,
    v.failure_tags,
    v.severity
FROM session_judges.verdicts v
JOIN session_judges.outcomes o ON o.id = v.outcome_id
JOIN sessions.sessions s ON s.id = v.session_id
WHERE v.mode = 'live'
ORDER BY v.session_id, v.judged_at DESC, v.id DESC;

COMMENT ON VIEW mike.session_quality_v IS
    'One row per session with a live verdict, carrying its LATEST live verdict and that row''s end '
    'label fields. Shadow and eval rows (JC13) are excluded. Set-based DISTINCT ON, no correlated '
    'scalar subquery.';

-- ── 3. Eval items for session-end runs ─────────────────────────────────────────────────────────
--
-- The session-end twin of JC10's turn-shaped eval.run_items. One row per ending turn in a
-- `mike eval run --role session_judge`. llm_end_label keeps the model's answer and rule_fired
-- records which deterministic rule's conditions held whether or not the run applied it, so the gate
-- scores four variants (model only, +fleet-seat-v1, +done-v1, both) from one run.

CREATE TABLE IF NOT EXISTS eval.session_end_run_items (
    run_id                   bigint      NOT NULL REFERENCES eval.runs(id) ON DELETE CASCADE,
    end_turn_id              bigint      NOT NULL REFERENCES memory.operator_turns(id) ON DELETE CASCADE,
    session_id               bigint      NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    label_id                 bigint      NOT NULL REFERENCES eval.labels(id),
    verdict_id               bigint      NULL REFERENCES session_judges.verdicts(id) ON DELETE SET NULL,
    outcome                  text        NOT NULL
                                         CHECK (outcome IN ('judged', 'refused', 'failed', 'skipped_unresolved')),
    label                    text        NOT NULL
                                         CHECK (label IN ('done', 'closed_in_frustration', 'cut_off_neutral',
                                                          'fleet_seat', 'unclear')),
    llm_end_label            text        NULL
                                         CHECK (llm_end_label IS NULL OR llm_end_label IN
                                                ('done', 'closed_in_frustration', 'cut_off_neutral',
                                                 'fleet_seat', 'unclear')),
    rule_fired               text        NULL CHECK (rule_fired IS NULL OR rule_fired IN ('fleet-seat-v1', 'done-v1')),
    effective_end_label      text        NULL,
    llm_frustration_correct  boolean     NULL,
    effective_correct        boolean     NULL,
    created_at               timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, end_turn_id),
    CONSTRAINT session_end_run_items_judged_check
        CHECK ((outcome = 'judged') = (verdict_id IS NOT NULL OR llm_end_label IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_eval_session_end_run_items_turn
    ON eval.session_end_run_items (end_turn_id);
CREATE INDEX IF NOT EXISTS idx_eval_session_end_run_items_session
    ON eval.session_end_run_items (session_id);
CREATE INDEX IF NOT EXISTS idx_eval_session_end_run_items_label
    ON eval.session_end_run_items (label_id);
CREATE INDEX IF NOT EXISTS idx_eval_session_end_run_items_verdict
    ON eval.session_end_run_items (verdict_id) WHERE verdict_id IS NOT NULL;

COMMENT ON TABLE eval.session_end_run_items IS
    'One row per ending turn in a session-end eval run: the label it was scored against, the '
    'session_judges.verdicts row (mode eval) holding the full input and output, the model''s own '
    'end label, the deterministic rule whose conditions held, and the effective label.';

-- ── 4. Closed lists widened (re-issue the list on main at build time, then append) ─────────────

-- JC12's re-judge runs: a session-end history re-judge (JC13 §4.8).
ALTER TABLE eval.rejudge_runs DROP CONSTRAINT IF EXISTS rejudge_runs_role_check;
ALTER TABLE eval.rejudge_runs ADD CONSTRAINT rejudge_runs_role_check
    CHECK (role IN ('turn_judge', 'session_judge'));

-- The session-end shadow path, its bundle role and its toggle are not built (a
-- pre-promotion soak, removed from JC13); judge_call_loop is already in the alarm kind
-- CHECK (20260925000400_semantic_context.sql).

-- ── 5. JC08's class map: production end label against the session_end label ────────────────────

INSERT INTO eval.class_map
    (production_axis, production_class, label_axis, label_value, replied_counts_as_negative)
VALUES
    ('session_end', 'done',                  'session_end', 'done',                  false),
    ('session_end', 'closed_in_frustration', 'session_end', 'closed_in_frustration', false),
    ('session_end', 'cut_off_neutral',       'session_end', 'cut_off_neutral',       false),
    ('session_end', 'fleet_seat',            'session_end', 'fleet_seat',            false),
    ('session_end', 'unclear',               'session_end', 'unclear',               false)
ON CONFLICT (production_axis, production_class) DO NOTHING;

-- ── 6. A floors-only gate has no baseline run ──────────────────────────────────────────────────
--
-- The legacy session judge has no end label, so there is no baseline run to compare a session-end
-- candidate with; the legacy detectors are scored by SQL inside the gate report instead. Omit this
-- statement if JC10 ships baseline_run_id nullable. eval_gates_distinct_runs_check still holds:
-- a NULL comparison passes a CHECK.
ALTER TABLE eval.gates ALTER COLUMN baseline_run_id DROP NOT NULL;

-- ── 7. A frustration close counts as a correction in the steering views ────────────────────────
--
-- After JC12 and JC13, `abandoned` is written only by the projection's frustration branch
-- (outcome_source = 'frustration_close'): the final turn of a session the operator closed in
-- frustration. "Corrected" in these two views now means "counts as a correction": corrected, or
-- such a frustration close. Column names and order are unchanged.

CREATE OR REPLACE VIEW steering.verifier_rate_v AS
SELECT
    p.project_path,
    count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL)              AS fires_count,
    count(i.id) FILTER (
        WHERE o.code = 'corrected'
           OR (o.code = 'abandoned' AND ot.outcome_source = 'frustration_close')
    )                                                                       AS corrected_count,
    count(i.id) FILTER (
        WHERE i.delivered_request_id IS NOT NULL
          AND ot.judge_failure_reason IS NULL
    )                                                                       AS delivered_count,
    CASE WHEN count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL) = 0
         THEN NULL::double precision
         ELSE round(
             count(i.id) FILTER (
                 WHERE o.code = 'corrected'
                    OR (o.code = 'abandoned' AND ot.outcome_source = 'frustration_close')
             )::numeric
             / count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL)::numeric, 4)::double precision
    END AS corrected_rate,
    CASE WHEN count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL) = 0
         THEN NULL::double precision
         ELSE round(
             count(i.id) FILTER (
                 WHERE i.delivered_request_id IS NOT NULL
                   AND ot.judge_failure_reason IS NULL
             )::numeric
             / count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL)::numeric, 4)::double precision
    END AS delivered_rate,
    count(i.id) FILTER (WHERE ot.judge_failure_reason IS NOT NULL)          AS excluded_judge_failed_count
FROM steering.interventions i
JOIN sessions.sessions s ON s.id = i.session_id
JOIN projects.projects p ON p.id = s.project_id
LEFT JOIN (
    SELECT operator_turns.session_id,
           operator_turns.start_request_id,
           operator_turns.end_request_id,
           operator_turns.outcome_id,
           operator_turns.outcome_source,
           operator_turns.judge_failure_reason
      FROM memory.operator_turns
    OFFSET 0
) ot ON ot.session_id = i.session_id
       AND ot.start_request_id <= i.trigger_request_id
       AND ot.end_request_id >= i.trigger_request_id
LEFT JOIN memory.turn_outcomes o ON o.id = ot.outcome_id
WHERE i.steerer_id IN (
    SELECT steerers.id FROM steering.steerers WHERE steerers.kind = 'verifier'
)
GROUP BY p.project_path;

COMMENT ON VIEW steering.verifier_rate_v IS
    'Per project: verifier fires, and how many landed on a turn that counts as a correction '
    '(outcome corrected, or abandoned from a frustration close: outcome_source = frustration_close, '
    'JC13). delivered_count as before.';

-- eir_timing_v: corrections include frustration closes, and the fires CTE selects the verifier by
-- kind. Its old filter, st.name = 'false-done', has matched no steerer since the rename
-- (20260826000001); on 2026-09-24 the view returned 481 correction rows and no fire rows.
CREATE OR REPLACE VIEW steering.eir_timing_v AS
WITH turn_ordinals AS (
    SELECT
        ot.id                AS turn_id,
        ot.session_id,
        ot.start_request_id,
        ot.end_request_id,
        ot.outcome_id,
        ot.outcome_source,
        ot.classified_at,
        row_number() OVER (PARTITION BY ot.session_id ORDER BY ot.start_request_id) AS turn_ordinal,
        count(*) OVER (PARTITION BY ot.session_id) AS session_turn_count
    FROM memory.operator_turns ot
),
fires AS (
    SELECT
        i.id                 AS intervention_id,
        i.session_id,
        i.trigger_request_id,
        i.created_at         AS fired_at,
        t.turn_id            AS fire_turn_id,
        t.turn_ordinal        AS fire_turn_ordinal,
        t.session_turn_count
    FROM steering.interventions i
    JOIN steering.steerers st ON st.id = i.steerer_id AND st.kind = 'verifier'
    JOIN turn_ordinals t
        ON t.session_id = i.session_id
       AND t.start_request_id <= i.trigger_request_id
       AND t.end_request_id >= i.trigger_request_id
),
corrections AS (
    SELECT
        t.turn_id            AS correction_turn_id,
        t.session_id,
        t.turn_ordinal        AS correction_turn_ordinal,
        t.classified_at       AS corrected_at,
        t.session_turn_count
    FROM turn_ordinals t
    JOIN memory.turn_outcomes o ON o.id = t.outcome_id
    WHERE o.code = 'corrected'
       OR (o.code = 'abandoned' AND t.outcome_source = 'frustration_close')
),
fire_correction_pairs AS (
    SELECT
        f.intervention_id, f.session_id, f.trigger_request_id, f.fired_at,
        f.fire_turn_id, f.fire_turn_ordinal, f.session_turn_count,
        c.correction_turn_id, c.correction_turn_ordinal, c.corrected_at,
        row_number() OVER (
            PARTITION BY f.intervention_id
            ORDER BY abs(c.correction_turn_ordinal - f.fire_turn_ordinal), c.correction_turn_ordinal
        ) AS rn
    FROM fires f
    LEFT JOIN corrections c ON c.session_id = f.session_id
),
correction_fire_pairs AS (
    SELECT
        c.correction_turn_id, c.session_id, c.correction_turn_ordinal, c.corrected_at, c.session_turn_count,
        f.intervention_id, f.fire_turn_id, f.fire_turn_ordinal, f.fired_at, f.trigger_request_id,
        row_number() OVER (
            PARTITION BY c.correction_turn_id
            ORDER BY f.fire_turn_ordinal DESC NULLS LAST
        ) AS rn
    FROM corrections c
    LEFT JOIN fires f ON f.session_id = c.session_id AND f.fire_turn_ordinal <= c.correction_turn_ordinal
)
SELECT
    'fire'::text                                    AS row_kind,
    intervention_id, session_id, trigger_request_id,
    fired_at                                        AS event_at,
    fire_turn_id, fire_turn_ordinal,
    correction_turn_id, correction_turn_ordinal, corrected_at,
    (correction_turn_ordinal - fire_turn_ordinal)   AS signed_gap,
    CASE
        WHEN correction_turn_ordinal IS NULL THEN 'never'
        WHEN correction_turn_ordinal - fire_turn_ordinal > 0 THEN 'early'
        WHEN correction_turn_ordinal - fire_turn_ordinal = 0 THEN 'on-time'
        ELSE 'late'
    END                                              AS timing_class,
    session_turn_count,
    CASE WHEN session_turn_count < 8 THEN 'short' ELSE 'long' END AS session_length_segment
FROM fire_correction_pairs
WHERE rn = 1
UNION ALL
SELECT
    'correction'::text                              AS row_kind,
    intervention_id, session_id, trigger_request_id,
    fired_at                                        AS event_at,
    fire_turn_id, fire_turn_ordinal,
    correction_turn_id, correction_turn_ordinal, corrected_at,
    (correction_turn_ordinal - fire_turn_ordinal)   AS signed_gap,
    CASE
        WHEN intervention_id IS NULL THEN 'no_fire_preceded'
        ELSE 'fire_preceded'
    END                                              AS timing_class,
    session_turn_count,
    CASE WHEN session_turn_count < 8 THEN 'short' ELSE 'long' END AS session_length_segment
FROM correction_fire_pairs
WHERE rn = 1;

COMMENT ON VIEW steering.eir_timing_v IS
    'Per verifier fire: signed operator-turn gap to the nearest turn that counts as a correction '
    '(corrected, or a frustration close, JC13) (early/on-time/late/never). Per such turn: whether '
    'a fire preceded it. row_kind discriminates the two; session_length_segment splits short '
    '(<8 turns) vs long (>=8) sessions.';
