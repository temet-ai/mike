UPDATE requests.messages rm
   SET cache_control = v.cache_control
  FROM (SELECT UNNEST(ARRAY[262692::bigint, 262692::bigint]) AS request_id,
               UNNEST(ARRAY[0::int, 1::int]) AS ord,
               UNNEST(ARRAY['{"pointer":"/content/0/cache_control"}'::jsonb,
                            '{"pointer":"/content/0/cache_control"}'::jsonb]) AS cache_control) v
 WHERE rm.request_id = v.request_id
   AND rm.ord = v.ord
   AND rm.cache_control IS NULL
