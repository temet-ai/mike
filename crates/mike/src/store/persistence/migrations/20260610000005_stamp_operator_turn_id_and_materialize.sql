-- Stamp requests.operator_turn_id and materialize closed-turn aggregates
-- (thread: turn-prompt-extraction, 2026-06-10).
--
-- Background: nothing in the Rust binary ever stamped
-- `requests.operator_turn_id`, and the compute-once-at-close
-- materialization of the aggregate cache columns on
-- `memory.operator_turns` (tool_call_count … final_assistant_text) was
-- never implemented after the schema squash. Consequences:
--   - `operator_turn_aggregates_v` returned 0 / '{}' for EVERY turn
--     (closed turns COALESCE never-written cache columns; the open turn
--     live-computes over `r.operator_turn_id` members that never existed);
--   - `operator_turn_text_v` returned NULL final_assistant_text for
--     closed turns, so `get_turn_for_embedding` embedded prompts only;
--   - every operator-turn DTO consumer (REST, MCP recall, SPA panels)
--     saw zero tools/tokens/files.
--
-- The Rust fix stamps membership at the turn lifecycle (open / extend /
-- abort-replacement) and materializes the cache inside the close
-- transaction. This migration repairs history:
--   1. Stamp every request into its turn by range membership
--      (same session, id within [start_request_id, end_request_id] —
--      ranges are sequential per session by construction).
--   2. Materialize the cache columns for every closed turn, mirroring
--      the live-turn arms of operator_turn_aggregates_v exactly.
--
-- Cost note: plain column updates + aggregates over normalized tables;
-- no request_body parsing (the failure mode that blocked startup for
-- 120 s in an earlier backfill is structurally absent here).

-- Step 1: membership stamp by range.
UPDATE requests.requests r
SET operator_turn_id = t.id
FROM memory.operator_turns t
WHERE r.session_id = t.session_id
  AND r.id >= t.start_request_id
  AND r.id <= t.end_request_id
  AND r.operator_turn_id IS NULL;

-- Step 2: materialize the aggregate cache for closed turns that have
-- never been materialized (tool_call_count IS NULL distinguishes them —
-- the Rust close path always writes the full column set).
UPDATE memory.operator_turns t
SET tool_call_count = (
        SELECT COUNT(*) FROM tools.uses u
        JOIN requests.requests r ON r.id = u.request_id
        WHERE r.operator_turn_id = t.id),
    tool_error_count = (
        SELECT COUNT(*) FROM tools.results res
        JOIN requests.requests r ON r.id = res.request_id
        WHERE r.operator_turn_id = t.id AND res.is_error),
    weasel_findings_count = (
        SELECT COUNT(*) FROM requests.diagnostics d
        JOIN requests.requests r ON r.id = d.request_id
        WHERE r.operator_turn_id = t.id AND d.analyzer = 'weasel'),
    tools_used = COALESCE((
        SELECT ARRAY_AGG(DISTINCT u.tool_name ORDER BY u.tool_name)
        FROM tools.uses u
        JOIN requests.requests r ON r.id = u.request_id
        WHERE r.operator_turn_id = t.id AND u.tool_name IS NOT NULL), '{}'),
    agents_dispatched = COALESCE((
        SELECT ARRAY_AGG(DISTINCT ag.name ORDER BY ag.name)
        FROM agents.dispatches ad
        JOIN tools.uses u ON u.id = ad.tool_use_id
        JOIN requests.requests r ON r.id = u.request_id
        JOIN agents.agents ag ON ag.id = ad.agent_id
        WHERE r.operator_turn_id = t.id), '{}'),
    files_touched = COALESCE((
        SELECT ARRAY_AGG(DISTINCT u.file_path ORDER BY u.file_path)
        FROM tools.uses u
        JOIN requests.requests r ON r.id = u.request_id
        WHERE r.operator_turn_id = t.id AND u.file_path IS NOT NULL), '{}'),
    total_input_tokens = COALESCE((
        SELECT SUM(resp.input_tokens)
        FROM responses.responses resp
        JOIN requests.requests r ON r.id = resp.request_id
        WHERE r.operator_turn_id = t.id), 0),
    total_output_tokens = COALESCE((
        SELECT SUM(resp.output_tokens)
        FROM responses.responses resp
        JOIN requests.requests r ON r.id = resp.request_id
        WHERE r.operator_turn_id = t.id), 0),
    final_assistant_text = (
        SELECT string_agg(c.chunk_content, '' ORDER BY c.request_id, c.seq_ord)
        FROM responses.stream_chunks c
        JOIN requests.requests r ON r.id = c.request_id
        JOIN responses.stream_delta_types dt ON dt.id = c.delta_type_id
        WHERE r.operator_turn_id = t.id AND dt.code = 'text')
WHERE t.closed_at IS NOT NULL
  AND t.tool_call_count IS NULL;
