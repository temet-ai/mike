-- Wright code repair (temet-ai/mike#75): one row per repair of a code defect an engineer run
-- traced to the part of mike that owns it.
--
-- A finding of kind code_defect with its cause established and an owning part gets one repair
-- (repairs_one_per_finding). The engineer edits a worktree of its own; the executor, which uses
-- no model, commits the patch as Wright, refuses changes to Wright, authentication, the gate,
-- credentials or deploy paths, gates the exact commit with the push hook and lands it on main.
--
-- States, in order: editing (the engineer run has the worktree), prepared (the patch is a
-- commit), gating (the push hook runs on that commit), landed (merged on main, merge_sha),
-- deployed (the live build's commit contains merge_sha), healthy (the problem's own check passed
-- after deployed). gate_failed and refused end an attempt and keep their reason; the branch is
-- kept, the worktree removed. Wright never deploys and never closes a problem itself.
--
-- gate_output is the push hook's whole output; gate_output_bytes is its size. An output over
-- the executor's limit is not kept (gate_output NULL, gate_output_bytes set), never clipped.

SET LOCAL lock_timeout = '10s';

CREATE TABLE wright.repairs (
    id           bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    finding_id   bigint      NOT NULL REFERENCES wright.findings(id) ON DELETE CASCADE,
    problem_id   bigint      NOT NULL REFERENCES wright.problems(id) ON DELETE CASCADE,
    owning_part  text        NOT NULL CHECK (btrim(owning_part) <> ''),
    state        text        NOT NULL CHECK (state IN ('editing', 'prepared', 'gating', 'gate_failed',
                                                       'refused', 'landed', 'deployed', 'healthy')),
    worktree     text        NOT NULL,
    branch       text        NOT NULL,
    branches     text[]      NOT NULL DEFAULT '{}',
    base_sha     text,
    commit_sha   text,
    gates        integer     NOT NULL DEFAULT 0 CHECK (gates >= 0),
    patch        jsonb       NOT NULL DEFAULT '{}'::jsonb,
    gate_exit    integer,
    gate_output  text,
    gate_output_bytes integer CHECK (gate_output_bytes >= 0),
    pr_url       text,
    merge_sha    text,
    reason       text,
    machine_uuid uuid,
    created_at   timestamptz NOT NULL DEFAULT now(),
    prepared_at  timestamptz,
    gated_at     timestamptz,
    landed_at    timestamptz,
    deployed_at  timestamptz,
    healthy_at   timestamptz,
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT repairs_one_per_finding UNIQUE (finding_id),
    CONSTRAINT repairs_ended_has_reason
        CHECK (state NOT IN ('gate_failed', 'refused') OR reason IS NOT NULL),
    CONSTRAINT repairs_landed_has_merge
        CHECK (state NOT IN ('landed', 'deployed', 'healthy')
               OR (merge_sha IS NOT NULL AND landed_at IS NOT NULL)),
    CONSTRAINT repairs_deployed_has_time
        CHECK (state NOT IN ('deployed', 'healthy') OR deployed_at IS NOT NULL),
    CONSTRAINT repairs_healthy_has_time
        CHECK ((state = 'healthy') = (healthy_at IS NOT NULL))
);

CREATE INDEX idx_wright_repairs_problem ON wright.repairs (problem_id, id DESC);
CREATE INDEX idx_wright_repairs_live
    ON wright.repairs (state, id)
    WHERE state IN ('editing', 'prepared', 'gating', 'landed', 'deployed');

COMMENT ON TABLE wright.repairs IS
    'Wright code repairs: one per established code-defect finding, gated by the push hook and landed by code that uses no model (temet-ai/mike#75).';
