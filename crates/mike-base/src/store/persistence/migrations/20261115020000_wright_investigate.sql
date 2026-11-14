-- Wright engineer runs (temet-ai/mike#73): one bounded, sandboxed Claude run at a time that
-- traces a problem with no known fix to the part of mike that owns it.
--
-- wright.engineer_limits: one row. The limits the supervisor enforces outside the model:
-- starts per UTC day, wall seconds per run, tool calls and tokens per run. Tool calls and
-- tokens are counted from captured traffic for the run's session; tokens are input, output
-- and cache-creation tokens (cache reads are not counted). Changed only through
-- PATCH /api/wright/engineer-limits, which an engineer run cannot reach.
-- wright.investigations: one row per run, written 'starting' (with host and machine) before
-- the process is spawned, so a crash between the write and the spawn leaves a row that the
-- next start reconciles. The partial unique index keeps one live run across processes,
-- machines and restarts. session_uuid is the Claude session the run uses.
-- wright.findings: what an investigate run concluded, validated before it is stored. A
-- finding either names the confirmed cause and the owning part, or says the cause was not
-- established; it always lists its evidence.
-- wright_session_uuids(): the sessions Wright itself started, so the measures Wright is
-- judged on and its checks can leave them out.
-- wright.has_enabled_remedy(check): whether a known fix answers a check. The remedies table
-- belongs to temet-ai/mike#74; the function reads it only when it exists.

SET LOCAL lock_timeout = '10s';

CREATE TABLE wright.engineer_limits (
    id           boolean     PRIMARY KEY DEFAULT true CHECK (id),
    daily_starts integer     NOT NULL CHECK (daily_starts BETWEEN 0 AND 24),
    wall_seconds integer     NOT NULL CHECK (wall_seconds BETWEEN 60 AND 7200),
    tool_calls   integer     NOT NULL CHECK (tool_calls BETWEEN 1 AND 2000),
    tokens       bigint      NOT NULL CHECK (tokens BETWEEN 10000 AND 50000000),
    updated_at   timestamptz NOT NULL DEFAULT now()
);

INSERT INTO wright.engineer_limits (daily_starts, wall_seconds, tool_calls, tokens)
VALUES (4, 1800, 300, 3000000)
ON CONFLICT (id) DO NOTHING;

CREATE TABLE wright.investigations (
    id           bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    mode         text        NOT NULL CHECK (mode IN ('investigate', 'repair', 'answer')),
    problem_id   bigint      REFERENCES wright.problems(id) ON DELETE SET NULL,
    episode_id   bigint      REFERENCES wright.episodes(id) ON DELETE SET NULL,
    session_uuid uuid        NOT NULL UNIQUE,
    state        text        NOT NULL
                             CHECK (state IN ('starting', 'running', 'finished', 'failed', 'killed', 'reconciled')),
    host         text        NOT NULL CHECK (btrim(host) <> ''),
    machine_uuid uuid,
    pid          integer     CHECK (pid > 0),
    created_at   timestamptz NOT NULL DEFAULT now(),
    started_at   timestamptz,
    deadline_at  timestamptz NOT NULL,
    ended_at     timestamptz,
    tool_calls   integer     NOT NULL DEFAULT 0 CHECK (tool_calls >= 0),
    tokens       bigint      NOT NULL DEFAULT 0 CHECK (tokens >= 0),
    exit_code    integer,
    end_reason   text,
    CONSTRAINT investigations_live_has_no_end CHECK ((state IN ('starting', 'running')) = (ended_at IS NULL)),
    CONSTRAINT investigations_running_has_pid CHECK (state <> 'running' OR pid IS NOT NULL),
    CONSTRAINT investigations_end_has_reason CHECK (state IN ('starting', 'running', 'finished') OR end_reason IS NOT NULL)
);

CREATE UNIQUE INDEX uq_wright_investigations_one_live
    ON wright.investigations ((true)) WHERE state IN ('starting', 'running');
CREATE INDEX idx_wright_investigations_created ON wright.investigations (created_at DESC);
CREATE INDEX idx_wright_investigations_problem
    ON wright.investigations (problem_id, id DESC) WHERE problem_id IS NOT NULL;
CREATE INDEX idx_wright_investigations_episode
    ON wright.investigations (episode_id) WHERE episode_id IS NOT NULL;

CREATE TABLE wright.findings (
    id                bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    investigation_id  bigint      NOT NULL UNIQUE REFERENCES wright.investigations(id) ON DELETE CASCADE,
    problem_id        bigint      REFERENCES wright.problems(id) ON DELETE CASCADE,
    episode_id        bigint      REFERENCES wright.episodes(id) ON DELETE SET NULL,
    symptom           text        NOT NULL CHECK (btrim(symptom) <> ''),
    suspected_cause   text        NOT NULL CHECK (btrim(suspected_cause) <> ''),
    confirmed_cause   text        CHECK (btrim(confirmed_cause) <> ''),
    cause_established boolean     NOT NULL,
    owning_part       text        CHECK (btrim(owning_part) <> ''),
    kind              text        NOT NULL
                                  CHECK (kind IN ('code_defect', 'config', 'leftover', 'external', 'unknown')),
    remedy            text        NOT NULL CHECK (btrim(remedy) <> ''),
    evidence          jsonb       NOT NULL
                                  CHECK (jsonb_typeof(evidence) = 'array' AND jsonb_array_length(evidence) > 0),
    created_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT findings_established_names_cause_and_part
        CHECK (NOT cause_established OR (confirmed_cause IS NOT NULL AND owning_part IS NOT NULL)),
    CONSTRAINT findings_unestablished_confirms_nothing
        CHECK (cause_established OR confirmed_cause IS NULL)
);

CREATE INDEX idx_wright_findings_problem ON wright.findings (problem_id, id DESC) WHERE problem_id IS NOT NULL;
CREATE INDEX idx_wright_findings_episode ON wright.findings (episode_id) WHERE episode_id IS NOT NULL;

CREATE FUNCTION public.wright_session_uuids() RETURNS SETOF uuid
    LANGUAGE sql STABLE AS $$ SELECT session_uuid FROM wright.investigations $$;

CREATE FUNCTION wright.has_enabled_remedy(p_check text) RETURNS boolean
    LANGUAGE plpgsql STABLE AS $$
BEGIN
    IF to_regclass('wright.remedies') IS NULL THEN
        RETURN false;
    END IF;
    RETURN EXISTS (SELECT 1 FROM wright.remedies r WHERE r.check_name = p_check AND r.enabled);
END
$$;

COMMENT ON TABLE wright.investigations IS
    'Wright engineer runs: one sandboxed Claude run at a time, reconciled before a retry (temet-ai/mike#73).';
COMMENT ON TABLE wright.findings IS
    'What an engineer run concluded about a problem: symptom, causes, owning part and the evidence it relied on.';
COMMENT ON TABLE wright.engineer_limits IS
    'Limits enforced outside the model on every engineer run; one row, changed only by the operator.';
