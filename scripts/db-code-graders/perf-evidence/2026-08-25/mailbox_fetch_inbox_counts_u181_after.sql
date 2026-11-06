SELECT
    COUNT(*) FILTER (
        WHERE to_session_id = 1
          AND consumed_at IS NULL
          AND dead_lettered_at IS NULL
          AND sent_at IS NOT NULL
    ) AS "unconsumed!",
    COUNT(*) FILTER (
        WHERE to_session_id = 1 AND claimed_at IS NULL
    ) AS "unclaimed!",
    COUNT(*) FILTER (
        WHERE from_session_id = 1 AND dead_lettered_at IS NOT NULL
    ) AS "dead_lettered_outbox!"
FROM mailbox.messages
WHERE to_session_id = 1 OR from_session_id = 1;
