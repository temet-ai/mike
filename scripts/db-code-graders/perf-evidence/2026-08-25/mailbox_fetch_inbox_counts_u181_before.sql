SELECT
    COUNT(*) FILTER (
        WHERE consumed_at IS NULL
          AND dead_lettered_at IS NULL
          AND sent_at IS NOT NULL
    ) AS "unconsumed!",
    COUNT(*) FILTER (WHERE claimed_at IS NULL) AS "unclaimed!"
FROM mailbox.messages
WHERE to_session_id = 1;
