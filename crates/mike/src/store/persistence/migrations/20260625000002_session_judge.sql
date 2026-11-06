-- ════════════════════════════════════════════════════════════════════
-- GitHub Issue #24 — R2: per-session LLM-judge outcome / quality
--
-- Adds a lightweight judge subsystem that records an LLM-derived
-- outcome verdict and quality score for each judged session.
--
-- Schema layout:
--   session_judges.outcomes   — lookup: unknown, success, partial, failure
--   session_judges.verdicts   — one row per judge run per session;
--                               re-runs append rather than overwrite
--
-- Read surfaces:
--   claude_proxy.session_quality_v  — latest verdict per session
--                                    (DISTINCT ON — set-based, no correlated
--                                     subquery per sql-set-based.md)
--   claude_proxy.session_summary_v  — extended with three nullable columns
--                                    sourced via LEFT JOIN to session_quality_v
--
-- Feature toggle: 'session_judge' seeded enabled=true (no-default-off rule).
-- ════════════════════════════════════════════════════════════════════

-- ── 1. Schema ─────────────────────────────────────────────────────────────────

CREATE SCHEMA IF NOT EXISTS session_judges;

-- ── 2. Outcomes lookup table ──────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS session_judges.outcomes (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text   NOT NULL UNIQUE
);

-- Seed in deterministic id order (OVERRIDING SYSTEM VALUE mirrors prompt_kinds
-- pattern). Order: unknown first so id=1 is the safe default.
INSERT INTO session_judges.outcomes (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'unknown'),
    (2, 'success'),
    (3, 'partial'),
    (4, 'failure')
ON CONFLICT (id) DO NOTHING;

-- ── 3. Verdicts fact table ────────────────────────────────────────────────────
--
-- Grain: one row per judge run per session. Re-runs under the same
-- (session_id, judge_model, rubric_version) APPEND a new row rather than
-- overwriting; the latest-verdict view uses DISTINCT ON to surface the most
-- recent. This design preserves the full judge history for auditing.
--
-- quality_score is nullable: the judge may produce an outcome without a
-- numeric score (e.g. 'unknown'), or the rubric may not require one.
-- rationale is nullable: short free-text justification from the judge.
-- Full text stored per inspection-fidelity.md — never truncated.

CREATE TABLE IF NOT EXISTS session_judges.verdicts (
    id             bigint          GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id     bigint          NOT NULL
                                   REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    outcome_id     bigint          NOT NULL
                                   REFERENCES session_judges.outcomes(id),
    quality_score  numeric(4,3)    NULL
                                   CHECK (quality_score IS NULL
                                       OR (quality_score >= 0 AND quality_score <= 1)),
    rationale      text            NULL,
    judge_model    text            NOT NULL,
    rubric_version integer         NOT NULL CHECK (rubric_version >= 1),
    judged_at      timestamptz     NOT NULL DEFAULT now(),
    created_at     timestamptz     NOT NULL DEFAULT now()
);

-- Index every FK column (project convention, mandatory).
CREATE INDEX idx_sj_verdicts_session_id
    ON session_judges.verdicts USING btree (session_id);

CREATE INDEX idx_sj_verdicts_outcome_id
    ON session_judges.verdicts USING btree (outcome_id);

-- Primary read path: latest verdict per session (feeds session_quality_v).
CREATE INDEX idx_sj_verdicts_session_judged_at
    ON session_judges.verdicts USING btree (session_id, judged_at DESC);

-- "Has this session been judged under this model+rubric?" probe.
-- NOT UNIQUE: re-runs under the same (session_id, judge_model, rubric_version)
-- must be allowed to append a fresh verdict row (history preserved).
CREATE INDEX idx_sj_verdicts_session_model_rubric
    ON session_judges.verdicts USING btree (session_id, judge_model, rubric_version);

-- ── 4. Latest-verdict-per-session view ───────────────────────────────────────
--
-- One row per session that has at least one verdict, carrying its LATEST one.
-- Set-based DISTINCT ON (no correlated scalar subquery, per sql-set-based.md).
-- session_id exposed as text to match session_summary_v's keying convention
-- (s.session_uuid::text).

CREATE OR REPLACE VIEW claude_proxy.session_quality_v AS
SELECT DISTINCT ON (v.session_id)
    s.session_uuid::text AS session_id,
    o.code               AS outcome,
    v.quality_score,
    v.rationale,
    v.judge_model,
    v.rubric_version,
    v.judged_at
FROM session_judges.verdicts v
JOIN session_judges.outcomes  o ON o.id = v.outcome_id
JOIN sessions.sessions        s ON s.id = v.session_id
ORDER BY v.session_id, v.judged_at DESC, v.id DESC;

COMMENT ON VIEW claude_proxy.session_quality_v IS
    'One row per session that has been judged, carrying its LATEST verdict. '
    'Uses DISTINCT ON (session_id) ORDER BY judged_at DESC, id DESC — set-based, '
    'no correlated scalar subquery. session_id is text (session_uuid::text) to '
    'align with session_summary_v keying. Joins outcomes for the code label. '
    'Sessions with no verdict row are excluded (caller LEFT JOINs if needed).';

-- ── 5. Extend session_summary_v ───────────────────────────────────────────────
--
-- Re-issue CREATE OR REPLACE VIEW keeping every existing column unchanged
-- (Rust DTOs map by name; column ORDER must be preserved to avoid positional
-- drift). Three new nullable columns appended via LEFT JOIN to session_quality_v.
--
-- The existing view uses GROUP BY (s.id, p.id, mdl.last_models). session_quality_v
-- is 1:1 per session_id (it is already deduplicated by DISTINCT ON), so the LEFT
-- JOIN introduces at most one matching row per s. The join key is text
-- (s.session_uuid::text = sq.session_id), which is safe because session_uuid is
-- NOT NULL on sessions.sessions (confirmed in baseline migration). The sq.*
-- columns are added to the GROUP BY so the query remains valid under standard
-- SQL aggregation rules.

CREATE OR REPLACE VIEW claude_proxy.session_summary_v AS
SELECT s.session_uuid::text                        AS session_id,
       p.project_path,
       s.first_seen,
       s.last_seen,
       s.last_status,
       s.title,
       s.pinned,
       p.pinned                                    AS project_pinned,
       COUNT(r.id)::int4                           AS request_count,
       COALESCE(mdl.last_models, ARRAY[]::text[]) AS last_models,
       -- New: latest judge verdict columns (NULL when session has not been judged).
       sq.outcome                                  AS session_outcome,
       sq.quality_score                            AS session_quality_score,
       sq.judged_at                                AS session_judged_at
FROM sessions.sessions s
JOIN projects.projects p ON p.id = s.project_id
LEFT JOIN requests.requests r ON r.session_id = s.id
LEFT JOIN LATERAL (
    SELECT ARRAY_AGG(model ORDER BY first_seen) AS last_models
    FROM (
        SELECT r2.model, MIN(r2.created_at) AS first_seen
        FROM requests.requests r2
        WHERE r2.session_id = s.id
          AND r2.model IS NOT NULL
        GROUP BY r2.model
    ) sub
) mdl ON true
LEFT JOIN claude_proxy.session_quality_v sq
       ON sq.session_id = s.session_uuid::text
GROUP BY s.id, p.id, mdl.last_models, sq.outcome, sq.quality_score, sq.judged_at;

COMMENT ON VIEW claude_proxy.session_summary_v IS
    'One row per session with its project-joined summary metadata. '
    'request_count via LEFT JOIN GROUP BY (one pass over requests). '
    'last_models: deduped list of models in chronological first-occurrence order '
    '(MIN(created_at) per model inside a LATERAL derived table — eliminates the '
    'correlated scalar subquery that previously ran once per session row on every '
    'session-list page load). '
    'last_path dropped (always /v1/messages). '
    'Latency computed on demand against idx_requests_session_completed. '
    'session_outcome / session_quality_score / session_judged_at: nullable columns '
    'sourced from claude_proxy.session_quality_v via LEFT JOIN — NULL when the '
    'session has not yet been judged by the LLM judge (GitHub issue #24).';

-- ── 6. Feature toggle: session_judge (enabled=true, no-default-off rule) ──────
--
-- config.feature_toggles.key carries a closed CHECK constraint
-- (introduced in 20260615000003_feature_toggles.sql, last extended in
-- 20260621000002_feature_toggle_db_resync.sql). The re-add below whitelists
-- BOTH new eval-harness toggle keys ('session_judge' here and 'judge_worker'
-- from the sibling judge-labeling migration) so the two siblings are
-- order-independent: whichever applies second re-adds the identical CHECK and
-- drops neither key. Idempotent: DROP IF EXISTS + ON CONFLICT DO NOTHING.

ALTER TABLE config.feature_toggles
    DROP CONSTRAINT IF EXISTS feature_toggles_key_check;

ALTER TABLE config.feature_toggles
    ADD CONSTRAINT feature_toggles_key_check
    CHECK (key IN (
        'reminders',
        'recall_injector',
        'weasel_intercept',
        'steerer',
        'lesson_worker',
        'harvester',
        'db_resync',
        'judge_worker',
        'session_judge'
    ));

-- Seed the new toggle enabled=true (no-default-off rule). Idempotent.
-- Only 'session_judge' is seeded here; the sibling judge-labeling migration
-- seeds 'judge_worker'. The CHECK above already permits both so the two compose.
INSERT INTO config.feature_toggles (key, enabled) VALUES
    ('session_judge', true)
ON CONFLICT (key) DO NOTHING;
