INSERT INTO agents.work_item_attention (session_id, kind, subject, since, message)
VALUES (1545259, 'main_idle_with_open_work', 'main@1791344235', '2026-10-07 03:37:15.353531+00', 'The main thread has been idle since 2026-10-07 03:37:15.353531 UTC while 1 agent(s) and 0 background job(s) are still open. Check the ledger and decide what to resume or stop.')
ON CONFLICT (session_id, kind, subject) DO NOTHING
RETURNING id
