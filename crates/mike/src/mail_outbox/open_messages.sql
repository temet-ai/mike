WITH ready AS (
    SELECT o.message_uuid
    FROM mailbox.outbox o
    LEFT JOIN mailbox.messages m ON m.message_uuid = o.message_uuid
    LEFT JOIN mailbox.runtime_notifications n ON n.message_id = m.id
    WHERE o.status IN ('pending', 'unresolved')
      AND (n.message_id IS NULL OR m.consumed_at IS NOT NULL OR m.dead_lettered_at IS NOT NULL)
    ORDER BY o.updated_at, o.queued_at, o.message_uuid
    LIMIT $1
    FOR UPDATE OF o SKIP LOCKED
), touched AS (
    UPDATE mailbox.outbox o SET updated_at = clock_timestamp()
    FROM ready WHERE o.message_uuid = ready.message_uuid
    RETURNING o.message_uuid
)
SELECT message_uuid FROM touched
