-- Fix tool_error_count to count DISTINCT errored tool CALLS scoped by the
-- EMITTING use's turn — not error-result rows scoped by the carrying request.
--
-- Background: the previous formula joined tools.results by request_id and
-- counted result rows.  Two distinct defects compounded:
--   1. Re-quote inflation: a tool call re-quoted into N subsequent requests
--      within a turn produced N error-result rows for one call.
--   2. Carrying-request mis-scoping: tools.results.request_id is the request
--      that CARRIED the result back, which may belong to a LATER turn than the
--      one that EMITTED the tool call.  Scoping the count by the carrying
--      request therefore credited turn t with errors emitted in earlier turns
--      whose results were re-quoted into t's requests.
-- Turn 3580, for example, showed 7 but had only 1 distinct errored call of
-- its own — the other 6 were earlier turns' errors re-quoted into 3580.
--
-- Correct semantics (matches fetch_turn_tool_calls_paired, the per-turn
-- tool-calls endpoint): how many tool_use rows EMITTED in this turn's requests
-- (tools.uses.request_id → requests.operator_turn_id) have an error result?
-- That is:
--     COUNT(DISTINCT u.id)
--       FROM tools.uses u
--       JOIN requests.requests r  ON r.id = u.request_id
--       JOIN tools.results res    ON res.tool_use_id = u.id
--      WHERE r.operator_turn_id = t.id AND res.is_error
-- The join r.id = u.request_id is the emitting-turn scope; COUNT(DISTINCT u.id)
-- dedups the re-quoted result rows.
--
-- Performance: the per-turn subquery drives idx_requests_operator_turn_id →
-- idx_tool_uses_request_id → idx_tool_results_tool_use_id.  The full
-- ~3 055-turn backfill measures ~5 s on the live DB (within the migration
-- budget; statement_timeout lifted to 120 s as a guard).

-- Part 1: Redefine the live-turn arm of operator_turn_aggregates_v.
--
-- The la LATERAL previously joined tools.results by res.request_id = r.id
-- (carrying-request scope) and counted res.tool_use_id.  That arm now joins
-- results by res.tool_use_id = u.id (emitting-use scope) and counts u.id —
-- the open-turn analogue of the corrected subquery above.  The result join is
-- dedicated to tool_error_count; the other la columns (tools_used,
-- weasel_findings_count, files_touched, agents_dispatched) are unaffected.
--
-- Idempotent: CREATE OR REPLACE VIEW.
CREATE OR REPLACE VIEW claude_proxy.operator_turn_aggregates_v AS
SELECT ot.id AS turn_id,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.tool_call_count,       0) ELSE COALESCE(la.tool_call_count,       0) END AS tool_call_count,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.tool_error_count,      0) ELSE COALESCE(la.tool_error_count,      0) END AS tool_error_count,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.weasel_findings_count, 0) ELSE COALESCE(la.weasel_findings_count, 0) END AS weasel_findings_count,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.total_input_tokens,    0)::numeric ELSE COALESCE(lt.total_input_tokens,  0) END AS total_input_tokens,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.total_output_tokens,   0)::numeric ELSE COALESCE(lt.total_output_tokens, 0) END AS total_output_tokens,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.tools_used,         '{}') ELSE COALESCE(la.tools_used,         '{}') END AS tools_used,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.agents_dispatched,  '{}') ELSE COALESCE(la.agents_dispatched,  '{}') END AS agents_dispatched,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.files_touched,      '{}') ELSE COALESCE(la.files_touched,      '{}') END AS files_touched
FROM memory.operator_turns ot
-- la: non-token aggregates for the open turn, parameterized per outer row.
-- tools.uses and diagnostics are independent 1:N children of requests; the
-- results join now keys off the use (res.tool_use_id = u.id) so an errored
-- call is counted under the turn that EMITTED it, never the turn whose request
-- merely re-quoted the result. COUNT(DISTINCT) / ARRAY_AGG(DISTINCT) give
-- correct totals despite the uses x diagnostics fan-out. agents.dispatches
-- joins through tools.uses (1:1).
LEFT JOIN LATERAL (
    SELECT COUNT(DISTINCT u.id)                                                                                     AS tool_call_count,
           COALESCE(ARRAY_AGG(DISTINCT u.tool_name ORDER BY u.tool_name) FILTER (WHERE u.tool_name IS NOT NULL), '{}'::text[]) AS tools_used,
           COUNT(DISTINCT CASE WHEN res.is_error THEN u.id END)                                                     AS tool_error_count,
           COUNT(DISTINCT CASE WHEN d.analyzer = 'weasel' THEN d.id END)                                           AS weasel_findings_count,
           COALESCE(ARRAY_AGG(DISTINCT ag.name ORDER BY ag.name) FILTER (WHERE ag.name IS NOT NULL), '{}'::text[])  AS agents_dispatched,
           COALESCE(ARRAY_AGG(DISTINCT u.file_path ORDER BY u.file_path) FILTER (WHERE u.file_path IS NOT NULL), '{}'::text[]) AS files_touched
    FROM requests.requests r
    LEFT JOIN tools.uses u ON u.request_id = r.id
    LEFT JOIN tools.results res ON res.tool_use_id = u.id
    LEFT JOIN requests.diagnostics d ON d.request_id = r.id
    LEFT JOIN agents.dispatches ad ON ad.tool_use_id = u.id
    LEFT JOIN agents.agents ag ON ag.id = ad.agent_id
    WHERE r.operator_turn_id = ot.id
      AND ot.closed_at IS NULL
) la ON true
-- lt: token sums only. responses.responses is 1:1 per request so SUM is
-- exact. Kept separate from la to avoid the uses x diagnostics fan-out
-- inflating the sum.
LEFT JOIN LATERAL (
    SELECT COALESCE(SUM(resp.input_tokens),  0) AS total_input_tokens,
           COALESCE(SUM(resp.output_tokens), 0) AS total_output_tokens
    FROM requests.requests r
    LEFT JOIN responses.responses resp ON resp.request_id = r.id
    WHERE r.operator_turn_id = ot.id
      AND ot.closed_at IS NULL
) lt ON true;

-- Part 2: Backfill tool_error_count for all closed turns with the corrected
-- emitting-use formula.  Re-materializes only tool_error_count (the other
-- aggregate columns are already correct).  Idempotent: re-running sets the
-- same value.  This also repairs any wrong values written by an earlier
-- carrying-request backfill (e.g. turn 3580 7 → 1).
SET statement_timeout = '120s';

UPDATE memory.operator_turns t
SET tool_error_count = (
    SELECT COUNT(DISTINCT u.id)
    FROM tools.uses u
    JOIN requests.requests r ON r.id = u.request_id
    JOIN tools.results res ON res.tool_use_id = u.id
    WHERE r.operator_turn_id = t.id AND res.is_error)
WHERE t.closed_at IS NOT NULL;
