-- fetch_thread_core_by_uuid (crates/mike/src/store/persistence/mailbox.rs) —
-- the threading lookup on the post_session_message hot path
-- (resolve_thread_for_send resolving a caller-supplied thread_id to its
-- identity). Measured against 2000 synthetic mailbox.threads rows seeded in a
-- rolled-back transaction (schema not yet applied to the live DB at
-- measurement time — see commit body for methodology). $1 substituted with a
-- representative seeded thread_uuid via the inline subquery.
SELECT id, thread_uuid, subject FROM mailbox.threads
 WHERE thread_uuid = (SELECT thread_uuid FROM mailbox.threads ORDER BY id LIMIT 1)
