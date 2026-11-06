WITH ins AS (
    INSERT INTO agents.work_item_attention (session_id, kind, subject, since, message)
    VALUES (1545259, 'main_idle_with_open_work', 'main@1791344235', '2026-10-07 03:37:15.353531+00', 'The main thread has been idle since 2026-10-07 03:37:15.353531 UTC while 1 agent(s) and 0 background job(s) are still open. Check the ledger and decide what to resume or stop.')
    ON CONFLICT (session_id, kind, subject) DO NOTHING
    RETURNING id, delivery_attempts, last_delivery_attempt_at
)
SELECT ins.id AS id, true AS inserted, false AS delivered, ins.delivery_attempts AS delivery_attempts, ins.last_delivery_attempt_at AS last_delivery_attempt_at
  FROM ins
UNION ALL
SELECT a.id, false, a.delivered_message_uuid IS NOT NULL, a.delivery_attempts, a.last_delivery_attempt_at
  FROM agents.work_item_attention a
 WHERE a.session_id = 1545259 AND a.kind = 'main_idle_with_open_work' AND a.subject = 'main@1791344235'
   AND NOT EXISTS (SELECT 1 FROM ins)
