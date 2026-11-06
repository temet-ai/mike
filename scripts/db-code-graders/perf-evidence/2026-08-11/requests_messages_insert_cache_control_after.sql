INSERT INTO requests.messages (request_id, ord, message_id, cache_control)
 SELECT u.request_id, u.ord, u.message_id, u.cache_control
 FROM UNNEST(ARRAY[262643]::bigint[], ARRAY[999000]::int[], ARRAY[8014371]::bigint[], ARRAY['[{"pointer":"/cache_control","value":{"type":"ephemeral"}}]'::jsonb]::jsonb[])
      AS u(request_id, ord, message_id, cache_control);
