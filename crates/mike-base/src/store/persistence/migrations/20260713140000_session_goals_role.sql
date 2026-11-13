-- fleet-canvas rebuild: a session's `role` is DECLARED, never inferred from
-- goal/title text (that laundering was rejected — a goal paragraph is not a
-- role). Free-text, open set — NOT an enum, NOT a CHECK constraint: real
-- agent_dispatches.subagent_type values already span 20+ definitions
-- (rust-developer, frontend-developer, db-dev, ui-designer, researcher,
-- verifier, java-dev, general-purpose, ...) and a spawned peer session
-- (coordinator, rust-dev, frontend-dev, reviewer, alignment, observer) has
-- no dispatch row at all, so it declares its own role through this column.

ALTER TABLE sessions.session_goals
    ADD COLUMN role text;

COMMENT ON COLUMN sessions.session_goals.role IS
    'Free-text role the session declares for itself (e.g. "rust-developer", "coordinator") — open set, no enum. NULL when undeclared. GET /api/fleet/graph resolves a node''s role from this column first, falling back to agents.agents.name (agent_dispatches.subagent_type) only when the session IS a dispatched subagent, never from goal or title.';
