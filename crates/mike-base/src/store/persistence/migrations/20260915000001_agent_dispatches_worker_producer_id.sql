-- Add worker_producer_id to agents.dispatches — additive, idempotent.
--
-- Wire addition W3 ("one node per agent, explicitly"): when a captured
-- Agent-tool result carries the harness's own `agentId: <id>` line (the id
-- the spawned worker later announces itself with — on its own next
-- request's `x-claude-code-agent-id` header, or in a SendMessage `to:`
-- field), `extract::agent_result` parses it and
-- `request_write_worker_producer::write_worker_producer_ids` records it
-- here against the dispatch whose `tool_use_id` matches the result's wire
-- tool-use id. `session_flow::dispatches` reads it back to link the
-- dispatch directly to the same `worker:<session>:<producer>` actor id a
-- later request's header would mint, so the frontend's grouping.ts merges
-- the header worker, the instance worker and the dispatch into one agent
-- node instead of three separate ones.
--
-- Nullable: most dispatches never resolve a producer id — no Agent tool
-- result was captured at all, or the captured result carried no parseable
-- `agentId: ` line. NULL means "not yet known", not "known absent".
ALTER TABLE agents.dispatches ADD COLUMN IF NOT EXISTS worker_producer_id TEXT NULL;

COMMENT ON COLUMN agents.dispatches.worker_producer_id IS
    'The agentId the captured Agent tool result reported for this dispatch, '
    'parsed from the tool_result text by extract::agent_result. NULL until '
    'a matching Agent-tool tool_result is captured and successfully parsed.';
