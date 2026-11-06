INSERT INTO mailbox.messages (message_uuid, to_session_id, from_session_id, body)
SELECT '6f1f0a62-0000-4000-8000-0000000000a1', s.id, NULL, '[mike work registry] The main thread has been idle since 2026-10-07 03:37:15.353531 UTC while 1 agent(s) and 0 background job(s) are still open. Check the ledger and decide what to resume or stop.'
  FROM sessions.sessions s
 WHERE s.session_uuid = '054e744e-92d3-4f4e-b041-570e167206ed'
RETURNING queued_at
