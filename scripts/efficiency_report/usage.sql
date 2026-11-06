WITH selected AS (
    SELECT id FROM sessions.sessions
    WHERE session_uuid IN (SELECT value::uuid FROM jsonb_array_elements_text(:'sessions'::jsonb))
), cohort AS (
    SELECT r.id FROM requests.requests r JOIN selected s ON s.id = r.session_id
    WHERE r.created_at >= :'since'::timestamptz AND r.created_at <= :'until'::timestamptz
), usage AS (
    SELECT r.id, count(p.id) AS response_rows,
           bool_and(p.input_tokens IS NOT NULL AND p.output_tokens IS NOT NULL
                    AND p.cache_read_input_tokens IS NOT NULL AND p.cache_creation_input_tokens IS NOT NULL
                    AND p.input_tokens >= 0 AND p.output_tokens >= 0
                    AND p.cache_read_input_tokens >= 0 AND p.cache_creation_input_tokens >= 0) AS complete,
           CASE WHEN count(p.id) = 1 THEN
               sum(greatest(p.input_tokens,0) + greatest(p.output_tokens,0)
                   + greatest(p.cache_read_input_tokens,0) + greatest(p.cache_creation_input_tokens,0))
               ELSE 0 END AS observed_tokens
    FROM cohort r LEFT JOIN responses.responses p ON p.request_id = r.id GROUP BY r.id
)
SELECT count(*) AS requests, count(*) FILTER (WHERE response_rows = 1 AND complete) AS complete_requests,
       count(*) FILTER (WHERE response_rows <> 1 OR NOT complete) AS incomplete_requests,
       sum(observed_tokens) AS observed_tokens,
       (SELECT count(*) FROM selected) AS resolved_sessions
FROM usage
