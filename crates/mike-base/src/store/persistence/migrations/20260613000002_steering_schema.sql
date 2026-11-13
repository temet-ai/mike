-- Phase 19 steering Phase 1: `steering` schema, lookup tables, steerers,
-- enrolments, interventions, two read views, and turn_origins id=10.
--
-- All FKs are inline on their columns — steering is created after every
-- referenced schema (sessions, requests, responses, memory, agents) so
-- every reference is a back-reference to an already-existing table.
-- No cycle exists (nothing in those schemas points into steering), so
-- zero deferred ALTER TABLE additions are needed.
--
-- Baseline: verified max turn_origins id = 9 ('web_fetch_sidecar',
-- migration 20260610000002). id=10 is clean.

-- ─── Schema ───────────────────────────────────────────────────────────────
CREATE SCHEMA IF NOT EXISTS steering;

-- ─── Lookup tables (closed sets) ──────────────────────────────────────────
-- Pattern: OVERRIDING SYSTEM VALUE + setval, matching baseline:688-691.

CREATE TABLE steering.authorities (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO steering.authorities (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'supply_context'),
    (2, 'redirect'),
    (3, 'command');
SELECT setval('steering.authorities_id_seq', 3);

CREATE TABLE steering.modes (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO steering.modes (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'augment'),
    (2, 'drive');
SELECT setval('steering.modes_id_seq', 2);

CREATE TABLE steering.verdicts (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO steering.verdicts (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'fired'),
    (2, 'suppressed_debounce'),
    (3, 'suppressed_arbitration'),
    (4, 'no_drift'),
    (5, 'gate_miss'),
    (6, 'failed_closed');
SELECT setval('steering.verdicts_id_seq', 6);

-- ─── steerers — the steerer registry ──────────────────────────────────────
-- One row per named steerer (Refocus, Re-orienter, Memory keeper).
-- The steerer's prompt/definition lives in agents.agents + overrides.overrides
-- (decision: reuse existing agent-prompt infrastructure, not a new store).
-- `config` JSONB is justified for per-steerer operational tuning
-- (confidence_threshold, debounce_turns, command allowlist) — these are
-- schemaless per-steerer parameters that the inspect API does not filter/sort.
-- Every typed, queried fact (authority, mode, model, priority, enabled) is a
-- typed column beside the JSONB.

CREATE TABLE steering.steerers (
    id              bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uuid            uuid DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    name            text NOT NULL UNIQUE,
    display_name    text NOT NULL,
    description     text NOT NULL,
    agent_id        bigint NOT NULL
                      REFERENCES agents.agents(id) ON DELETE RESTRICT,
    authority_id    bigint NOT NULL
                      REFERENCES steering.authorities(id) ON DELETE RESTRICT,
    default_mode_id bigint NOT NULL
                      REFERENCES steering.modes(id) ON DELETE RESTRICT,
    model           text NOT NULL,
    priority        integer NOT NULL,
    enabled         boolean DEFAULT false NOT NULL,
    config          jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at      timestamptz DEFAULT now() NOT NULL,
    updated_at      timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT steerers_priority_nonneg CHECK (priority >= 0)
);
CREATE INDEX idx_steerers_agent_id  ON steering.steerers USING btree (agent_id);
CREATE INDEX idx_steerers_authority ON steering.steerers USING btree (authority_id);
CREATE INDEX idx_steerers_mode      ON steering.steerers USING btree (default_mode_id);
CREATE INDEX idx_steerers_enabled   ON steering.steerers USING btree (id) WHERE (enabled = true);

-- ─── enrolments — per-session opt-in ──────────────────────────────────────
-- One row per (session, steerer) enrolment. ADR constraint 1: per-session
-- opt-in required; no steerer fires without an enrolment row.

CREATE TABLE steering.enrolments (
    id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id     bigint NOT NULL
                     REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    steerer_id     bigint NOT NULL
                     REFERENCES steering.steerers(id) ON DELETE CASCADE,
    mode_id        bigint NOT NULL
                     REFERENCES steering.modes(id) ON DELETE RESTRICT,
    authority_id   bigint NOT NULL
                     REFERENCES steering.authorities(id) ON DELETE RESTRICT,
    enrolled_at    timestamptz DEFAULT now() NOT NULL,
    enrolled_by    text NOT NULL,
    disenrolled_at timestamptz,
    CONSTRAINT uq_enrolment_session_steerer UNIQUE (session_id, steerer_id)
);
CREATE INDEX idx_enrolments_session_id ON steering.enrolments USING btree (session_id);
CREATE INDEX idx_enrolments_steerer_id ON steering.enrolments USING btree (steerer_id);
CREATE INDEX idx_enrolments_mode       ON steering.enrolments USING btree (mode_id);
CREATE INDEX idx_enrolments_authority  ON steering.enrolments USING btree (authority_id);
-- Hot path: "which steerers is this live session enrolled in, right now?"
CREATE INDEX idx_enrolments_active     ON steering.enrolments USING btree (session_id)
                                       WHERE (disenrolled_at IS NULL);

-- ─── interventions — THE attribution row (bigbro) ─────────────────────────
-- One row per evaluation the steerer ran (fired or not). When it fires it
-- links to both the captured request AND the captured response.
-- The three nullable FKs (sent_request_id, sent_response_id, operator_turn_id)
-- are back-filled by UPDATE after the proxy capture path writes them — the same
-- three-stage pattern used by agents.instances.first_request_id (baseline:732,740).

CREATE TABLE steering.interventions (
    id                 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uuid               uuid DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    steerer_id         bigint NOT NULL
                         REFERENCES steering.steerers(id) ON DELETE RESTRICT,
    enrolment_id       bigint NOT NULL
                         REFERENCES steering.enrolments(id) ON DELETE CASCADE,
    session_id         bigint NOT NULL
                         REFERENCES sessions.sessions(id) ON DELETE CASCADE,

    -- What triggered it
    verdict_id         bigint NOT NULL
                         REFERENCES steering.verdicts(id) ON DELETE RESTRICT,
    mode_id            bigint NOT NULL
                         REFERENCES steering.modes(id) ON DELETE RESTRICT,
    authority_id       bigint NOT NULL
                         REFERENCES steering.authorities(id) ON DELETE RESTRICT,
    trigger_request_id bigint
                         REFERENCES requests.requests(id) ON DELETE SET NULL,
    gate_reason        text,
    off_thread_topic   text,
    confidence         double precision,
    verdict_json       jsonb,
    model              text,
    anchor_text        text,

    -- What it produced (populated only when verdict = fired, id=1)
    payload_text       text,
    sent_request_id    bigint
                         REFERENCES requests.requests(id) ON DELETE SET NULL,
    sent_response_id   bigint
                         REFERENCES responses.responses(id) ON DELETE SET NULL,
    operator_turn_id   bigint
                         REFERENCES memory.operator_turns(id) ON DELETE SET NULL,

    -- Staleness / idle-session tracking
    sent_at            timestamptz,
    processed_at       timestamptz,
    expires_at         timestamptz,
    reverted_at        timestamptz,
    revert_reason      text,

    created_at         timestamptz DEFAULT now() NOT NULL,

    CONSTRAINT interventions_confidence_range
        CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 1)),
    -- verdict id=1 is 'fired'; a fired intervention must carry the payload it sent.
    CONSTRAINT interventions_fired_has_payload
        CHECK (verdict_id <> 1 OR payload_text IS NOT NULL)
);

-- Per-FK indexes (db-dev rule: every FK column gets a btree index).
CREATE INDEX idx_interventions_steerer_id         ON steering.interventions USING btree (steerer_id);
CREATE INDEX idx_interventions_enrolment_id       ON steering.interventions USING btree (enrolment_id);
-- Composite index for the dominant read path: per-session intervention timeline.
CREATE INDEX idx_interventions_session_created    ON steering.interventions USING btree (session_id, created_at DESC);
CREATE INDEX idx_interventions_verdict            ON steering.interventions USING btree (verdict_id);
CREATE INDEX idx_interventions_mode               ON steering.interventions USING btree (mode_id);
CREATE INDEX idx_interventions_authority          ON steering.interventions USING btree (authority_id);
-- Partial indexes on back-filled nullable FKs: only fired interventions have these non-NULL.
CREATE INDEX idx_interventions_trigger_request_id ON steering.interventions USING btree (trigger_request_id)
                                                  WHERE (trigger_request_id IS NOT NULL);
CREATE INDEX idx_interventions_sent_request_id    ON steering.interventions USING btree (sent_request_id)
                                                  WHERE (sent_request_id IS NOT NULL);
CREATE INDEX idx_interventions_sent_response_id   ON steering.interventions USING btree (sent_response_id)
                                                  WHERE (sent_response_id IS NOT NULL);
CREATE INDEX idx_interventions_operator_turn_id   ON steering.interventions USING btree (operator_turn_id)
                                                  WHERE (operator_turn_id IS NOT NULL);
-- Partial index for the staleness sweep (ADR open question 4).
CREATE INDEX idx_interventions_pending            ON steering.interventions USING btree (session_id, expires_at)
                                                  WHERE (sent_at IS NOT NULL AND processed_at IS NULL);
-- BRIN on created_at: near-free on this append-mostly table, serves time-range scans.
CREATE INDEX brin_interventions_created_at        ON steering.interventions USING brin (created_at)
                                                  WITH (pages_per_range = '32');

-- ─── turn_origins: id=10 'steered' ────────────────────────────────────────
-- Max id confirmed = 9 ('web_fetch_sidecar') at migration-authoring time.
-- Pattern copied from 20260610000002 lines 27-29.
INSERT INTO requests.turn_origins (id, code) OVERRIDING SYSTEM VALUE VALUES (10, 'steered')
ON CONFLICT DO NOTHING;
SELECT setval('requests.turn_origins_id_seq', 10);

-- ─── Read views (in claude_proxy schema, full-content) ────────────────────

-- (a) steered_requests_v — per-request steered drill-down.
-- Drives the request-detail attribution panel. Only fired-and-captured
-- interventions (sent_request_id IS NOT NULL). Full text columns throughout.
CREATE VIEW claude_proxy.steered_requests_v AS
SELECT
    i.sent_request_id          AS request_id,
    i.sent_response_id         AS response_id,
    r.session_id,
    s.session_uuid,
    r.operator_turn_id,
    st.name                    AS steerer_name,
    st.display_name            AS steerer_display_name,
    auth.code                  AS authority,
    md.code                    AS mode,
    v.code                     AS verdict,
    i.off_thread_topic,
    i.confidence,
    i.payload_text,
    i.anchor_text,
    i.verdict_json,
    i.model,
    i.sent_at,
    i.processed_at,
    i.expires_at,
    i.reverted_at,
    i.revert_reason,
    i.created_at
FROM steering.interventions i
JOIN requests.requests r       ON r.id = i.sent_request_id
JOIN sessions.sessions s       ON s.id = i.session_id
JOIN steering.steerers st      ON st.id = i.steerer_id
JOIN steering.authorities auth ON auth.id = i.authority_id
JOIN steering.modes md         ON md.id = i.mode_id
JOIN steering.verdicts v       ON v.id = i.verdict_id
WHERE i.sent_request_id IS NOT NULL;

-- (b) session_steering_v — per-session intervention timeline.
-- Shows every evaluation (fires, suppressions, no-drift) with full reasoning.
CREATE VIEW claude_proxy.session_steering_v AS
SELECT
    i.id                AS intervention_id,
    i.session_id,
    s.session_uuid,
    p.project_path,
    st.name             AS steerer_name,
    st.display_name     AS steerer_display_name,
    v.code              AS verdict,
    auth.code           AS authority,
    md.code             AS mode,
    i.trigger_request_id,
    i.sent_request_id,
    i.sent_response_id,
    i.operator_turn_id,
    i.gate_reason,
    i.off_thread_topic,
    i.confidence,
    i.anchor_text,
    i.verdict_json,
    i.payload_text,
    i.model,
    i.sent_at,
    i.processed_at,
    i.expires_at,
    i.reverted_at,
    i.revert_reason,
    i.created_at
FROM steering.interventions i
JOIN sessions.sessions s       ON s.id = i.session_id
JOIN projects.projects p       ON p.id = s.project_id
JOIN steering.steerers st      ON st.id = i.steerer_id
JOIN steering.verdicts v       ON v.id = i.verdict_id
JOIN steering.authorities auth ON auth.id = i.authority_id
JOIN steering.modes md         ON md.id = i.mode_id;

-- (c) session_enrolments_v — enrolment badge (header chip).
CREATE VIEW claude_proxy.session_enrolments_v AS
SELECT
    e.id                AS enrolment_id,
    e.session_id,
    s.session_uuid,
    st.id               AS steerer_id,
    st.name             AS steerer_name,
    st.display_name     AS steerer_display_name,
    st.enabled          AS steerer_enabled,
    md.code             AS mode,
    auth.code           AS authority,
    e.enrolled_at,
    e.enrolled_by,
    e.disenrolled_at
FROM steering.enrolments e
JOIN sessions.sessions s       ON s.id = e.session_id
JOIN steering.steerers st      ON st.id = e.steerer_id
JOIN steering.modes md         ON md.id = e.mode_id
JOIN steering.authorities auth ON auth.id = e.authority_id;
