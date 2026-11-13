-- Jev decisions (temet-ai/mike milestone 8, #212, #213, #220, #221).
--
-- llm.calls gains the role 'decision' for calls to OpenRouter's Decisions endpoint, the
-- outcome 'schema_error' for a reply that did not parse into typed answers, and
-- response_body, which keeps the typed answers (or the unparsed reply on a schema error) so
-- a replay can join answers to outcomes. llm.daily_spend's role CHECK is widened with
-- llm.calls', because its trigger copies NEW.role.
--
-- Every change is additive: a binary older than this migration keeps serving.

SET LOCAL lock_timeout = '10s';

ALTER TABLE llm.calls DROP CONSTRAINT IF EXISTS calls_role_check;
ALTER TABLE llm.calls
    ADD CONSTRAINT calls_role_check
        CHECK (role IN ('steerer', 'session_judge', 'judge_worker', 'lesson_relations',
                        'eval', 'lesson_proposer', 'decision'));

ALTER TABLE llm.daily_spend DROP CONSTRAINT IF EXISTS daily_spend_role_check;
ALTER TABLE llm.daily_spend
    ADD CONSTRAINT daily_spend_role_check
        CHECK (role IN ('steerer', 'session_judge', 'judge_worker', 'lesson_relations',
                        'eval', 'lesson_proposer', 'decision'));

ALTER TABLE llm.calls DROP CONSTRAINT IF EXISTS calls_outcome_check;
ALTER TABLE llm.calls
    ADD CONSTRAINT calls_outcome_check
        CHECK (outcome IN ('ok', 'http_error', 'timeout', 'truncated', 'transport_error',
                           'refused_ceiling', 'refused_rate', 'refused_spend', 'schema_error'));

ALTER TABLE llm.calls ADD COLUMN IF NOT EXISTS response_body jsonb;

COMMENT ON COLUMN llm.calls.response_body IS
    'What the call answered, for calls that keep it (role decision: the typed answers, or the unparsed reply on schema_error).';

-- The decisions schema (#229) keeps everything Jev is asked and answers, verbatim, and the
-- labels that later say whether the answer was right. It is owned by the llm module of
-- mike-base, where the client writes it; other modules write labels through that module.
-- No sweep or prune touches it (crates/mike-base/src/llm/decisions/retention_tests.rs).
CREATE SCHEMA IF NOT EXISTS decisions;

CREATE TABLE decisions.records (
    id                bigserial   PRIMARY KEY,
    created_at        timestamptz NOT NULL DEFAULT now(),
    point             text        NOT NULL CHECK (point ~ '^[a-z][a-z0-9_]{1,62}$'),
    mode              text        NOT NULL CHECK (mode IN ('shadow', 'on', 'replay', 'cli')),
    question_set      text        NOT NULL,
    question_set_hash text        NOT NULL,
    thresholds        jsonb       NOT NULL DEFAULT '{}'::jsonb,
    model_requested   text        NOT NULL,
    state             jsonb       NOT NULL,
    questions         jsonb       NOT NULL,
    request_body      jsonb       NOT NULL,
    outcome           text        NOT NULL CHECK (outcome IN (
                          'ok', 'http_error', 'timeout', 'transport_error', 'schema_error',
                          'refused_ceiling', 'refused_rate', 'refused_spend', 'no_key')),
    error             text,
    response_raw      text,
    answers           jsonb,
    question_refusals integer     NOT NULL DEFAULT 0,
    served_model      text,
    provider          text,
    generation_id     text,
    usage             jsonb,
    cost_usd          double precision,
    latency_ms        integer,
    http_status       integer,
    composed          jsonb,
    applied           text        CHECK (applied IN ('sent', 'held', 'woken', 'warned', 'passed', 'none')),
    fail_open         boolean,
    facts_version     integer,
    facts             jsonb,
    session_id        bigint,
    operator_turn_id  bigint,
    request_id        bigint,
    evaluation_uuid   uuid,
    intervention_uuid uuid,
    fleet_run_id      bigint,
    seat              text,
    message_uuid      uuid,
    check_receipt_id  bigint,
    commit_sha        text,
    project_id        bigint,
    machine_uuid      uuid,
    source_ref        text
);

COMMENT ON TABLE decisions.records IS
    'One row per Jev decision call, for every outcome, with the verbatim state, questions, request and raw reply (#229). Never pruned.';

CREATE INDEX idx_decision_records_point_created
    ON decisions.records (point, created_at);
CREATE INDEX idx_decision_records_evaluation
    ON decisions.records (evaluation_uuid) WHERE evaluation_uuid IS NOT NULL;
CREATE UNIQUE INDEX idx_decision_records_source_ref
    ON decisions.records (source_ref) WHERE source_ref IS NOT NULL;

CREATE TABLE decisions.outcomes (
    id           bigserial   PRIMARY KEY,
    record_id    bigint      NOT NULL REFERENCES decisions.records (id),
    label        text        NOT NULL CHECK (btrim(label) <> ''),
    label_source text        NOT NULL CHECK (btrim(label_source) <> ''),
    evidence     jsonb       NOT NULL DEFAULT '{}'::jsonb,
    labelled_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (record_id, label_source, label)
);

-- One row per decisions_loop run (#220): what it scored per point, the findings it filed and
-- the outcomes it synced. The loop reads the newest row to know whether a run is overdue.
CREATE TABLE decisions.loop_runs (
    id            bigserial   PRIMARY KEY,
    started_at    timestamptz NOT NULL DEFAULT now(),
    finished_at   timestamptz,
    machine_uuid  uuid,
    report        jsonb       NOT NULL DEFAULT '{}'::jsonb
);

COMMENT ON TABLE decisions.loop_runs IS
    'decisions_loop runs: per-point scores, findings and synced outcomes (#220). Never pruned.';

-- A live turn-guard fire the pre-check held (#213) gets the arm 'precheck_held'. It was never
-- delivered, so it may carry an automatic label like a holdback row, and the labeller finds it
-- through its own partial index.
ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_arm_check;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_arm_check
        CHECK (arm IS NULL OR arm IN ('treated', 'holdback', 'precheck_held'));

ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_auto_label_shadow_only;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_auto_label_shadow_only
        CHECK (label_source IS NULL
               OR label_source NOT IN ('next_message', 'session_end')
               OR mode = 'shadow'
               OR arm IN ('holdback', 'precheck_held'));

CREATE INDEX IF NOT EXISTS idx_evaluations_unlabelled_precheck_held
    ON steering.evaluations (dispatched_at)
    WHERE outcome_label IS NULL AND arm = 'precheck_held' AND status <> 'dispatched';

COMMENT ON TABLE decisions.outcomes IS
    'Labels for decisions.records, many per record and never overwritten, so sources that disagree are all kept (#229). Never pruned.';

-- decisions_health (#221): today's Jev calls per point, their failures, schema errors,
-- refusals, spend and shadow volume; flags a point with no calls on a day it had traffic.
INSERT INTO wright.checks (name, source, cadence, expects, probe, query, params) VALUES
('decisions_health', 'capture_db', 'daily',
 'Every decision point that ran in the last day has calls that mostly succeed: failures under the share in params, and no point that ran yesterday silent today while its steerer still fires',
 'sql',
 $q$WITH today AS (
    SELECT point,
           count(*) AS calls,
           count(*) FILTER (WHERE outcome <> 'ok') AS failures,
           count(*) FILTER (WHERE outcome = 'schema_error') AS schema_errors,
           count(*) FILTER (WHERE outcome LIKE 'refused_%' OR outcome = 'no_key') AS refused,
           sum(question_refusals) AS question_refusals,
           count(*) FILTER (WHERE mode = 'shadow') AS shadow,
           COALESCE(sum(cost_usd), 0) AS cost_usd
      FROM decisions.records
     WHERE created_at >= now() - interval '1 day' AND mode IN ('shadow', 'on')
     GROUP BY point
),
before AS (
    SELECT DISTINCT point
      FROM decisions.records
     WHERE created_at >= now() - interval '2 days' AND created_at < now() - interval '1 day'
       AND mode IN ('shadow', 'on')
),
fires AS (
    SELECT count(*) AS n
      FROM steering.evaluations e
      JOIN steering.steerers s ON s.id = e.steerer_id
     WHERE s.name = 'turn-guard' AND e.dispatched_at >= now() - interval '1 day'
       AND e.status = 'resolved_correction'
)
SELECT 'decisions'::text AS scope,
       t.point AS resource,
       jsonb_build_object('calls', t.calls, 'failures', t.failures,
                          'schema_errors', t.schema_errors, 'refused', t.refused,
                          'question_refusals', t.question_refusals, 'shadow', t.shadow,
                          'cost_usd', round(t.cost_usd::numeric, 6)) AS evidence
  FROM today t
 WHERE t.failures::float8 / t.calls > ($1->>'max_failure_share')::float8
UNION ALL
SELECT 'decisions'::text, b.point,
       jsonb_build_object('calls', 0, 'live_fires_today', f.n,
                          'note', 'this point made decisions yesterday and none today')
  FROM before b CROSS JOIN fires f
 WHERE NOT EXISTS (SELECT 1 FROM today t WHERE t.point = b.point) AND f.n > 0$q$,
 '{"max_failure_share": 0.2}');
