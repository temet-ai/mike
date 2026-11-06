WITH injections AS MATERIALIZED (
    SELECT i.lesson_id, r.operator_turn_id, r.created_at AS captured_at
    FROM memory.lesson_injections i
    JOIN requests.requests r ON r.id = i.request_id
    JOIN memory.lessons l ON l.id = i.lesson_id
    LEFT JOIN projects.projects p ON p.id = l.project_id
    WHERE i.injected_at >= '2026-09-27T17:40:00Z'::timestamptz AND i.injected_at < '2026-10-04T17:40:00Z'::timestamptz
      AND (NULL::text IS NULL OR p.project_path = NULL)
), counts AS (
    SELECT lesson_id, count(*) AS recorded_injections FROM injections GROUP BY lesson_id
), turns AS MATERIALIZED (
    SELECT i.lesson_id, t.id AS turn_id, min(i.captured_at) AS exposed_at,
           t.reaction, t.delivery, f.checks_failed
    FROM injections i JOIN memory.operator_turns t ON t.id = i.operator_turn_id
    JOIN memory.turn_facts f ON f.operator_turn_id = t.id
    WHERE f.prompt_kind = 'genuine' AND f.text_chars > 0
    GROUP BY i.lesson_id, t.id, t.reaction, t.delivery, f.checks_failed
), outcomes AS (
    SELECT lesson_id, count(*) AS exposed_turns,
           count(*) FILTER (WHERE reaction IN ('accepted','corrected','redirected')) AS judged_turns,
           count(*) FILTER (WHERE reaction = 'corrected') AS corrected_turns,
           count(*) FILTER (WHERE reaction = 'accepted') AS accepted_turns,
           count(*) FILTER (WHERE delivery IN ('shipped','delivered')) AS delivered_turns,
           sum(checks_failed)::bigint AS failed_checks
    FROM turns GROUP BY lesson_id
), boundaries AS (
    SELECT turn_id, min(exposed_at) AS exposed_at FROM turns GROUP BY turn_id
), captured AS MATERIALIZED (
    SELECT r.id AS request_id, r.operator_turn_id, r.created_at,
           rsp.input_tokens, rsp.output_tokens, rsp.cache_read_input_tokens, rsp.cache_creation_input_tokens
    FROM boundaries b JOIN requests.requests r ON r.operator_turn_id = b.turn_id
    LEFT JOIN responses.responses rsp ON rsp.request_id = r.id
    WHERE r.created_at >= b.exposed_at AND r.created_at < '2026-10-04T17:40:00Z'::timestamptz
), tool_counts AS MATERIALIZED (
    SELECT c.request_id, count(*) AS tool_calls
    FROM captured c JOIN tools.uses u ON u.request_id = c.request_id GROUP BY c.request_id
), usage AS (
    SELECT t.lesson_id, sum(c.input_tokens)::bigint AS input_tokens,
           sum(c.output_tokens)::bigint AS output_tokens,
           sum(c.cache_read_input_tokens)::bigint AS cache_read_tokens,
           sum(c.cache_creation_input_tokens)::bigint AS cache_write_tokens, count(*) AS requests,
           count(*) FILTER (WHERE c.input_tokens IS NOT NULL AND c.output_tokens IS NOT NULL) AS usage_requests,
           sum(coalesce(tc.tool_calls,0))::bigint AS tool_calls
    FROM turns t JOIN captured c ON c.operator_turn_id = t.turn_id AND c.created_at >= t.exposed_at
    LEFT JOIN tool_counts tc ON tc.request_id = c.request_id GROUP BY t.lesson_id
)
SELECT c.lesson_id AS "lesson_id!", c.recorded_injections AS "recorded_injections!",
       coalesce(o.exposed_turns,0) AS "exposed_turns!",
       coalesce(o.judged_turns,0) AS "judged_turns!",
       coalesce(o.corrected_turns,0) AS "corrected_turns!",
       coalesce(o.accepted_turns,0) AS "accepted_turns!",
       coalesce(o.delivered_turns,0) AS "delivered_turns!",
       u.input_tokens, u.output_tokens, u.cache_read_tokens, u.cache_write_tokens, coalesce(u.usage_requests,0) AS "usage_requests!",
       coalesce(u.requests,0) AS "requests!", coalesce(u.tool_calls,0) AS "tool_calls!",
       coalesce(o.failed_checks,0) AS "whole_turn_failed_checks!"
FROM counts c LEFT JOIN outcomes o USING (lesson_id)
LEFT JOIN usage u USING (lesson_id)
ORDER BY c.lesson_id