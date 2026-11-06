INSERT INTO mailbox.messages (message_uuid, to_session_id, from_session_id, body)
SELECT '6f1f0a62-0000-4000-8000-0000000000a1', a.session_id, NULL, '[mike work registry] The main thread has been idle since 2026-10-07 03:37:15.353531 UTC while 1 agent(s) and 0 background job(s) are still open. Check the ledger and decide what to resume or stop.'
  FROM agents.work_item_attention a
 WHERE a.id = 2293649
ON CONFLICT (message_uuid) DO NOTHING
