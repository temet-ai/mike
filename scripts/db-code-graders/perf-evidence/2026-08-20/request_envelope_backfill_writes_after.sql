UPDATE requests.requests r
   SET request_envelope = v.envelope
  FROM (SELECT UNNEST(ARRAY[1::bigint, 2::bigint]) AS id,
               UNNEST(ARRAY['\x7b7d'::bytea, '\x7b7d'::bytea]) AS envelope) v
 WHERE r.id = v.id
