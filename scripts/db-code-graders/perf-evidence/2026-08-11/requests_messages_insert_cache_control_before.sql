INSERT INTO requests.messages (request_id, ord, message_id)
 SELECT u.request_id, u.ord, u.message_id
 FROM UNNEST(ARRAY[262643]::bigint[], ARRAY[999000]::int[], ARRAY[8014371]::bigint[])
      AS u(request_id, ord, message_id);
