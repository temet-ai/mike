WITH injections AS MATERIALIZED (
    SELECT i.lesson_id, r.operator_turn_id, r.session_id, r.created_at AS captured_at,
           (origin.code IN ('primary','primary_first') AND r.claude_code_agent_id IS NULL) AS primary_lane
    FROM memory.lesson_injections i
    JOIN requests.requests r ON r.id = i.request_id
    JOIN requests.turn_origins origin ON origin.id = r.turn_origin_id
    JOIN memory.lessons l ON l.id = i.lesson_id
    LEFT JOIN projects.projects p ON p.id = l.project_id
    WHERE i.injected_at >= $1 AND i.injected_at < $2
), counts AS (
    SELECT i.lesson_id, count(*) AS recorded_injections,
           count(*) FILTER (WHERE t.id IS NULL OR NOT i.primary_lane) AS unattributed_injections
    FROM injections i LEFT JOIN memory.operator_turns t ON t.id=i.operator_turn_id AND t.session_id=i.session_id
    GROUP BY i.lesson_id
), turns AS MATERIALIZED (
    SELECT i.lesson_id, t.id AS turn_id, t.session_id, min(i.captured_at) AS exposed_at,
           t.reaction, t.delivery, f.checks_failed
    FROM injections i JOIN memory.operator_turns t ON t.id = i.operator_turn_id AND t.session_id = i.session_id
    JOIN memory.turn_facts f ON f.operator_turn_id = t.id
    WHERE i.primary_lane AND f.prompt_kind = 'genuine' AND f.text_chars > 0
    GROUP BY i.lesson_id, t.id, t.session_id, t.reaction, t.delivery, f.checks_failed
), outcomes AS (
    SELECT lesson_id, count(*) AS exposed_turns,
           count(*) FILTER (WHERE reaction IN ('accepted','corrected','redirected')) AS judged_turns,
           count(*) FILTER (WHERE reaction = 'corrected') AS corrected_turns,
           count(*) FILTER (WHERE reaction = 'accepted') AS accepted_turns,
           count(*) FILTER (WHERE delivery IN ('shipped','delivered')) AS delivered_turns,
           sum(checks_failed)::bigint AS failed_checks
    FROM turns GROUP BY lesson_id
), captured AS MATERIALIZED (
    SELECT r.operator_turn_id, r.session_id, r.created_at, rsp.input_tokens, rsp.output_tokens,
           rsp.cache_read_input_tokens, rsp.cache_creation_input_tokens, tool_calls
    FROM requests.requests r
    JOIN requests.turn_origins origin ON origin.id=r.turn_origin_id
    LEFT JOIN responses.responses rsp ON rsp.request_id=r.id
    -- Count tools once per selected request via its indexed request_id; retain one usage row.
    CROSS JOIN LATERAL (
        SELECT count(*) AS tool_calls FROM tools.uses u WHERE u.request_id=r.id
    ) tc
    WHERE r.operator_turn_id = ANY(ARRAY(SELECT DISTINCT turn_id FROM turns))
      AND r.created_at >= $1 AND r.created_at < $2
      AND origin.code IN ('primary','primary_first') AND r.claude_code_agent_id IS NULL
), usage AS (
    SELECT t.lesson_id, sum(r.input_tokens)::bigint AS input_tokens,
           sum(r.output_tokens)::bigint AS output_tokens,
           sum(r.cache_read_input_tokens)::bigint AS cache_read_tokens,
           sum(r.cache_creation_input_tokens)::bigint AS cache_write_tokens, count(*) AS requests,
           count(*) FILTER (WHERE r.input_tokens IS NOT NULL AND r.output_tokens IS NOT NULL) AS usage_requests,
           sum(r.tool_calls)::bigint AS tool_calls
    FROM turns t JOIN captured r ON r.operator_turn_id=t.turn_id
         AND r.session_id=t.session_id AND r.created_at>=t.exposed_at
    GROUP BY t.lesson_id
)
SELECT c.lesson_id AS "lesson_id!", c.recorded_injections AS "recorded_injections!",
       c.unattributed_injections AS "unattributed_injections!",
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
