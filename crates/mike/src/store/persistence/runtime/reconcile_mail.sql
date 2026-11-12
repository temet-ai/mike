WITH settled AS (
    UPDATE mailbox.outbox o SET
        status = CASE WHEN m.consumed_at IS NOT NULL THEN 'delivered' ELSE 'failed' END,
        status_reason = CASE WHEN m.consumed_at IS NOT NULL THEN 'recipient acknowledged mailbox message' ELSE 'mailbox dead letter' END,
        updated_at = now()
    FROM mailbox.messages m
    WHERE o.message_uuid = m.message_uuid
      AND o.message_uuid IN (
        SELECT candidate.message_uuid FROM mailbox.outbox candidate
        JOIN mailbox.messages original ON original.message_uuid = candidate.message_uuid
        WHERE candidate.status IN ('pending', 'unresolved')
          AND (original.consumed_at IS NOT NULL OR original.dead_lettered_at IS NOT NULL)
        ORDER BY candidate.queued_at LIMIT 500
      )
    RETURNING o.message_uuid, o.status
), staged AS (
    INSERT INTO mailbox.outbox_stages (message_uuid, stage, evidence)
    SELECT message_uuid, 'consumed', 'recipient acknowledged mailbox message'
    FROM settled WHERE status = 'delivered'
    ON CONFLICT (message_uuid, stage) DO NOTHING
    RETURNING message_uuid
)
SELECT count(*)::bigint AS "count!" FROM settled
