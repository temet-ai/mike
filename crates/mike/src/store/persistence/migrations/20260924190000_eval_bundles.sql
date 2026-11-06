-- JC06 (judgement-context program): eval.bundles, the configuration stamp on every verdict.
--
-- Why: on 2026-08-31 the turn judge's model, rubric and input changed together and
-- nothing recorded which configuration produced which label; JUDGE_RUBRIC_VERSION is a
-- compiled constant while the prompt and model are live settings, and mark_turn_corrected
-- records no writer at all (critique defects 1 and 10; fleet-mirror issue-drafts.md
-- Drafts 5 and 9). A bundle is the complete effective configuration of one verdict
-- writer. A new row is created only when that configuration changes, and its id is
-- carried forward by RETURNING into the settings overlay and stamped on every verdict.
--
-- content_sha256 is data for display and for eval run records. It is deliberately not
-- indexed and not UNIQUE (sql-set-based.md); the current bundle of a role is found by
-- (role, subject_key), never by hash.
--
-- The stamp columns carry no foreign key, like llm.calls (20260829000002_llm_calls.sql):
-- a verdict write must never fail on a bundle, and two of these inserts are on hot
-- paths (insert_evaluation_dispatched before drain; insert_intervention, PROVISIONAL
-- ~4.0 ms). ADD COLUMN IF NOT EXISTS keeps this file compatible with JC13 and JC14,
-- which add the same columns if they land first.
--
-- Roles are the live verdict writers only: turn_judge, session_judge, steerer. The LLM
-- correction classifier and the keyword fast path cannot write on the live binary
-- (catalogue, binding correction 2) and get no role; JC12 deletes them as dead code.
--
-- lock_timeout: this file alters memory.operator_turns, steering.evaluations and
-- steering.interventions, which capture writes continuously. An ADD COLUMN waiting for
-- its ACCESS EXCLUSIVE lock queues every later write behind it, so this migration caps
-- the wait at 5 s instead of the migration connection's configured
-- PROXY_MIGRATION_LOCK_TIMEOUT_SECS (default 180 s, store/persistence/pool.rs:15-21):
-- the deploy fails fast and is retried rather than stalling capture. Once granted, the
-- lock is held for milliseconds (catalogue-only column adds on PostgreSQL 17).

SET LOCAL lock_timeout = '5s';

CREATE SCHEMA IF NOT EXISTS eval;

CREATE TABLE IF NOT EXISTS eval.bundles (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    role             text        NOT NULL
                     CHECK (role IN ('turn_judge', 'session_judge', 'steerer')),
    subject_key      text        NOT NULL DEFAULT '',
    origin           text        NOT NULL
                     CHECK (origin IN ('resolver', 'candidate')),
    status           text        NOT NULL
                     CHECK (status IN ('candidate', 'shadow', 'live', 'live_ungated', 'retired')),
    components       jsonb       NOT NULL,
    content_sha256   text        NOT NULL,
    build_sha        text        NOT NULL,
    build_dirty      boolean     NOT NULL DEFAULT false,
    cause            jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at       timestamptz NOT NULL DEFAULT now(),
    superseded_at    timestamptz,
    superseded_by_id bigint      REFERENCES eval.bundles(id),
    gate_run_id      bigint,
    promoted_at      timestamptz,
    override_reason  text,
    CHECK (role = 'steerer' OR subject_key = ''),
    CHECK (role <> 'steerer' OR subject_key <> '')
);

COMMENT ON TABLE eval.bundles IS
    'One row per effective configuration of a live verdict writer (turn judge, session judge, each steerer). Resolver rows are created when the configuration changes and superseded by the next; candidate rows are created by mike eval (JC10). Rows are never deleted.';
COMMENT ON COLUMN eval.bundles.components IS
    'The complete effective configuration, built by the same resolution functions the writer uses: model, full prompt text, output schema, rubric and input-builder fingerprints, thresholds, reasoning effort, token cap.';
COMMENT ON COLUMN eval.bundles.content_sha256 IS
    'SHA-256 of the canonical JSON of components. Data only: never indexed, never UNIQUE, never used to look a row up.';
COMMENT ON COLUMN eval.bundles.origin IS
    'resolver = created from live configuration and superseded by the resolver; candidate = created by mike eval, never touched by the resolver.';
COMMENT ON COLUMN eval.bundles.build_sha IS
    'The build that created the row. Not part of the bundle identity: code changes enter through the compiled texts and probe fingerprints in components.';
COMMENT ON COLUMN eval.bundles.cause IS
    'Why the row was created: trigger (startup, setting, steerer, composition), the config.change_log ids of that write, and the keys involved.';

CREATE INDEX IF NOT EXISTS bundles_role_subject_idx
    ON eval.bundles (role, subject_key, id DESC);

ALTER TABLE memory.operator_turns
    ADD COLUMN IF NOT EXISTS outcome_bundle_id bigint;
COMMENT ON COLUMN memory.operator_turns.outcome_bundle_id IS
    'eval.bundles.id (role turn_judge) of the judge configuration that set outcome_id. NULL for labels written before JC06, for writes made while bundle resolution was unavailable, and for any label from a writer with no bundle (the dead classifier and fast path). No FK: stamping must never fail a label write.';

ALTER TABLE session_judges.verdicts
    ADD COLUMN IF NOT EXISTS bundle_id bigint;
COMMENT ON COLUMN session_judges.verdicts.bundle_id IS
    'eval.bundles.id (role session_judge) in force when this verdict was produced. No FK, deliberately.';

ALTER TABLE steering.evaluations
    ADD COLUMN IF NOT EXISTS bundle_id bigint;
COMMENT ON COLUMN steering.evaluations.bundle_id IS
    'eval.bundles.id (role steerer) of the evaluated steerer when the evaluation was dispatched. No FK, deliberately (hot path).';

ALTER TABLE steering.interventions
    ADD COLUMN IF NOT EXISTS bundle_id bigint;
COMMENT ON COLUMN steering.interventions.bundle_id IS
    'eval.bundles.id (role steerer) of the steerer that fired. No FK, deliberately (insert_intervention is on a hot path).';

-- The session-quality read surface carries the verdict's bundle, so every
-- route that shows a session's judge verdict can name its configuration.
-- CREATE OR REPLACE VIEW may only append a column, which is what this does.
CREATE OR REPLACE VIEW mike.session_quality_v AS
SELECT DISTINCT ON (v.session_id)
    s.session_uuid::text AS session_id,
    o.code               AS outcome,
    v.quality_score,
    v.rationale,
    v.judge_model,
    v.rubric_version,
    v.judged_at,
    v.bundle_id
FROM session_judges.verdicts v
JOIN session_judges.outcomes  o ON o.id = v.outcome_id
JOIN sessions.sessions        s ON s.id = v.session_id
ORDER BY v.session_id, v.judged_at DESC, v.id DESC;
