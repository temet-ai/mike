SELECT r.model,
       count(*) AS requests,
       COALESCE(sum(resp.input_tokens), 0)::bigint AS input_tokens,
       COALESCE(sum(resp.output_tokens), 0)::bigint AS output_tokens,
       COALESCE(sum(resp.cache_creation_input_tokens), 0)::bigint AS cache_creation_tokens,
       COALESCE(sum(resp.cache_read_input_tokens), 0)::bigint AS cache_read_tokens,
       max(r.created_at) AS last_seen
  FROM responses.responses resp
  JOIN requests.requests r ON r.id = resp.request_id
 WHERE r.created_at > now() - make_interval(days => 30)
   AND resp.response_status >= 200 AND resp.response_status < 300
 GROUP BY r.model
 ORDER BY count(*) DESC;
