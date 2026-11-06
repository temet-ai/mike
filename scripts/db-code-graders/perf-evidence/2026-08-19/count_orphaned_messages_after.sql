SELECT count(*) AS "count!", COALESCE(sum(byte_size), 0)::bigint AS "total_bytes!"
FROM messages.messages m
WHERE NOT EXISTS (
    SELECT 1 FROM requests.messages rm WHERE rm.message_id = m.id
)
AND NOT EXISTS (
    SELECT 1 FROM requests.deltas rd WHERE rd.message_id = m.id
)
