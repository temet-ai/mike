-- New ledger vocabulary entry (issue #230 S3/A7): a duplicate process's
-- `end` call on a registration it doesn't own is rejected, not silently
-- ignored — this event kind makes that rejection diagnosable from the
-- ledger instead of only from a 409 the caller may not have logged.
INSERT INTO workspace.event_kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (6, 'end_rejected')
ON CONFLICT (id) DO NOTHING;
SELECT setval('workspace.event_kinds_id_seq', 6);
