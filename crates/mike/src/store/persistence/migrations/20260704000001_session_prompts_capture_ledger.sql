-- system_prompts.session_prompts: per-conversation-start capture ledger.
--
-- Grain: one row per conversation start — one main row per session, plus one
-- row per subagent invocation. Identified synchronously at the wire, no sha
-- keying, no instance dependency:
--   lane='main'     — the first main-lane (primary/primary_first) request in
--                      the session carrying a system array.
--   lane='subagent' — every subagent/workflow_subagent request with a system
--                      array and message_count == 1.
--
-- Additive only: system_prompts.system_prompts (the class-SHA dedup table)
-- and requests.requests are untouched by this migration.
--
-- blocks is jsonb because it holds the full verbatim wire system[] array —
-- a schemaless capture payload, not a set of domain objects to filter/join on.
--
-- content_sha256 is an informational "same as previous" marker (computed
-- over the volatile-stripped blocks) and is NEVER a dedup key or join
-- target — the surrogate id is always carried via session_id/request_id,
-- never re-derived by matching this hash. Per .claude/rules/sql-set-based.md
-- a sha column is never itself indexed or UNIQUE (random-leaf B-tree
-- bloat); it rides as an INCLUDE payload on idx_session_prompts_session_lane
-- below instead of getting its own index.

CREATE TABLE IF NOT EXISTS system_prompts.session_prompts (
    id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id        bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    request_id        bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE UNIQUE,
    lane              text NOT NULL,
    subagent_persona  text,
    agent_id          bigint REFERENCES agents.agents(id) ON DELETE SET NULL,
    instance_id       bigint REFERENCES agents.instances(id) ON DELETE SET NULL,
    blocks            jsonb NOT NULL,
    content_sha256    text,
    altered           boolean NOT NULL DEFAULT false,
    alteration_used   boolean NOT NULL DEFAULT false,
    created_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT session_prompts_lane_check CHECK (lane IN ('main', 'subagent'))
);

COMMENT ON TABLE system_prompts.session_prompts IS
    'Capture ledger: one row per conversation start (one main row per session, one row per subagent invocation). Distinct from system_prompts.system_prompts, which is the class-SHA dedup table.';
COMMENT ON COLUMN system_prompts.session_prompts.request_id IS
    'The conversation-start request that carried this system array. UNIQUE + ON DELETE CASCADE: this ledger row has no meaning once its owning request is gone.';
COMMENT ON COLUMN system_prompts.session_prompts.subagent_persona IS
    'Synchronous agent hint read off the wire before the agents.instances matcher binds — may be set even when agent_id/instance_id are still NULL.';
COMMENT ON COLUMN system_prompts.session_prompts.agent_id IS
    'Stamped by the agent matcher once it resolves the dispatch. NULL until matched or if matching never occurs; SET NULL preserves the ledger row if the agent definition is later deleted.';
COMMENT ON COLUMN system_prompts.session_prompts.instance_id IS
    'Stamped by the agent matcher once it resolves the sub-agent instance. NULL until matched; SET NULL preserves the ledger row if the instance record is later deleted.';
COMMENT ON COLUMN system_prompts.session_prompts.content_sha256 IS
    'Informational "same as previous" marker (sha256 over the volatile-stripped blocks). Never indexed directly, never a dedup key, never joined on, never used to re-derive an id — carried as an INCLUDE payload on idx_session_prompts_session_lane for index-only reads.';
COMMENT ON COLUMN system_prompts.session_prompts.altered IS
    'True when the composer spliced system[2] (main/PrimaryFirst lane only).';
COMMENT ON COLUMN system_prompts.session_prompts.alteration_used IS
    'Confirmed true once the response shows the splice took effect.';

-- Index every FK. session_id is covered by the (session_id, lane) composite
-- below (leftmost-column lookups use it); request_id's UNIQUE constraint
-- already creates its own btree index. content_sha256 rides along as an
-- INCLUDE payload — never its own index (sql-set-based.md hard ban on
-- indexing sha columns; INCLUDE-as-payload is the explicit carve-out).
CREATE INDEX idx_session_prompts_session_lane
    ON system_prompts.session_prompts (session_id, lane) INCLUDE (content_sha256);
CREATE INDEX idx_session_prompts_agent_id ON system_prompts.session_prompts (agent_id) WHERE agent_id IS NOT NULL;
CREATE INDEX idx_session_prompts_instance_id ON system_prompts.session_prompts (instance_id) WHERE instance_id IS NOT NULL;

-- At most one lane='main' row per session — enforces the "one main row per
-- session" grain at the database level, not just in application logic.
CREATE UNIQUE INDEX uq_session_prompts_one_main_per_session
    ON system_prompts.session_prompts (session_id)
    WHERE lane = 'main';
