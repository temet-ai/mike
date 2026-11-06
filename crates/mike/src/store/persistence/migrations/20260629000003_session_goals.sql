-- Session goals: one goal text per session, set by the steering subsystem or MCP.
--
-- 1:1 extension of sessions.sessions — the PK is the FK, so no separate
-- unique index is needed. Resolving a goal for evaluation is a single
-- indexed PK lookup via the UNIQUE session_uuid index on sessions.sessions.
CREATE TABLE IF NOT EXISTS sessions.session_goals (
    session_id bigint PRIMARY KEY
        REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    goal       text        NOT NULL,
    set_by     text        NOT NULL DEFAULT 'mcp',
    updated_at timestamptz NOT NULL DEFAULT now()
);
