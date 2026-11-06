-- Lateralize operator_turn_aggregates_v (thread: turn-prompt-extraction, 2026-06-10).
--
-- Why: the baseline shape computed the live-turn aggregates as two plain
-- GROUP BY derived tables joined `ON la.operator_turn_id = ot.id`. Postgres
-- cannot push an outer join qual into a GROUP BY subquery, so EVERY query
-- touching the view materialized the aggregate over the ENTIRE corpus
-- (all stamped requests x tools.uses x tools.results x diagnostics x
-- dispatches), then join-filtered down. The baseline comment claimed the
-- open turn "computes live ... through idx_requests_operator_turn_id" —
-- the planner never produces that plan for this shape.
--
-- This was invisible until 20260610000005 backfilled
-- requests.operator_turn_id: with the column NULL everywhere the GROUP BY
-- subqueries matched zero rows and full-view materialization was free.
-- After the backfill, a single-session operator-turns listing burned
-- ~14,500 buffers (2.4 s cold) in the GroupAggregate.
--
-- Fix: same columns, same semantics, but each derived table becomes a
-- LEFT JOIN LATERAL whose WHERE carries `r.operator_turn_id = ot.id AND
-- ot.closed_at IS NULL` — the parameterized per-turn shape
-- operator_turn_text_v already uses. The planner drives it through
-- idx_requests_operator_turn_id only for the rows the outer query
-- selects, and closed turns skip the scan entirely (the WHERE is false;
-- they read their materialized cache columns). Measured on the live DB:
-- 21-turn session listing went from ~14,500 buffers / 70 ms warm to
-- ~50 buffers / <1 ms.

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
-- tools.uses, tools.results, and diagnostics are each independent 1:N
-- children of requests, creating a uses x results x diagnostics
-- cross-product per request — COUNT(DISTINCT) / ARRAY_AGG(DISTINCT) give
-- correct totals despite the duplication. agents.dispatches joins through
-- tools.uses (1:1).
LEFT JOIN LATERAL (
    SELECT COUNT(DISTINCT u.id)                                                                                     AS tool_call_count,
           COALESCE(ARRAY_AGG(DISTINCT u.tool_name ORDER BY u.tool_name) FILTER (WHERE u.tool_name IS NOT NULL), '{}'::text[]) AS tools_used,
           COUNT(DISTINCT CASE WHEN res.is_error THEN res.id END)                                                   AS tool_error_count,
           COUNT(DISTINCT CASE WHEN d.analyzer = 'weasel' THEN d.id END)                                           AS weasel_findings_count,
           COALESCE(ARRAY_AGG(DISTINCT ag.name ORDER BY ag.name) FILTER (WHERE ag.name IS NOT NULL), '{}'::text[])  AS agents_dispatched,
           COALESCE(ARRAY_AGG(DISTINCT u.file_path ORDER BY u.file_path) FILTER (WHERE u.file_path IS NOT NULL), '{}'::text[]) AS files_touched
    FROM requests.requests r
    LEFT JOIN tools.uses u ON u.request_id = r.id
    LEFT JOIN tools.results res ON res.request_id = r.id
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
