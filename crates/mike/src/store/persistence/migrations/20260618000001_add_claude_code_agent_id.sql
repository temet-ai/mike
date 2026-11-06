-- Add claude_code_agent_id to requests.requests and register the
-- workflow_subagent turn origin.
--
-- 1. Column: captures the value of the x-claude-code-agent-id HTTP header sent
--    by Claude Code when spawning workflow sub-agents.  Nullable — not present
--    on ordinary requests.  No index: low-cardinality-per-session; not a join
--    key at scale.  No NOT NULL / no default / no backfill: expand-only.
--
-- 2. turn_origins seed: id=11 'workflow_subagent' — used by the Rust classifier
--    (TurnOrigin::WorkflowSubagent) and the frontend turn-origins.ts union.
--    Code string locked as 'workflow_subagent'.

ALTER TABLE requests.requests
    ADD COLUMN IF NOT EXISTS claude_code_agent_id text;

-- Register the new turn origin.
-- id=11: last assigned was id=10 ('steered', migration 20260613000002).
-- ON CONFLICT DO NOTHING makes this idempotent on re-run.
-- setval advances the identity sequence past the explicit id so future plain
-- INSERTs don't collide.
INSERT INTO requests.turn_origins (id, code) OVERRIDING SYSTEM VALUE VALUES
    (11, 'workflow_subagent')
ON CONFLICT DO NOTHING;
SELECT setval('requests.turn_origins_id_seq', 11);
