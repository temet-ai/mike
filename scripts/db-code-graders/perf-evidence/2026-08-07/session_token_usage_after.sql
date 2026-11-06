SELECT r.model,
       COUNT(*)::bigint                                           AS response_count,
       COALESCE(SUM(resp.input_tokens), 0)::bigint                AS input_tokens,
       COALESCE(SUM(resp.output_tokens), 0)::bigint               AS output_tokens,
       COALESCE(SUM(resp.cache_read_input_tokens), 0)::bigint     AS cache_read_tokens,
       COALESCE(SUM(resp.cache_creation_input_tokens), 0)::bigint AS cache_write_tokens
FROM requests.requests r
JOIN responses.responses resp ON resp.request_id = r.id
WHERE r.session_id = 669197
  AND r.model IS NOT NULL
GROUP BY r.model
ORDER BY r.model ASC
