-- Launch briefs travel as mail pushed over Remote Control, not as a
-- per-request attachment (operator review, 2026-09-29).
--
-- 20260928120000_launch_brief_notes.sql shipped a proxy stage that
-- re-attached a note-mode brief to every request, because Claude Code never
-- keeps proxy-injected text. The operator's objection: a brief should be
-- inserted once, and once acknowledged it no longer needs inserting. The
-- acknowledgement is also the clearest signal that the spawned session has
-- read its brief and started.
--
-- Now a note-mode launch types nothing. Once the session's Remote Control
-- stream is linked, the launcher calls POST /api/launches/{session}/brief,
-- which mails the brief from mike (from_session_id NULL) under the launch's
-- brief_note_uuid and pushes it over Remote Control. Claude Code frames that
-- push as a teammate's request to act on. A mike channel message, by
-- contrast, is framed as untrusted external data, and a session given its
-- brief that way acknowledged it and then refused to act (docs/
-- INJECTION_TRUST.md). So claim_channel_messages skips brief mail. The
-- launch's brief_read_at is that message's consumed_at, read by join, so
-- there is no new column on launch.launches.
--
-- The index serves the channel claim's per-message NOT EXISTS lookup by
-- brief_note_uuid; it is partial because most launches have no brief.

SET LOCAL lock_timeout = '5s';

CREATE INDEX IF NOT EXISTS idx_launch_launches_brief_note_uuid
    ON launch.launches (brief_note_uuid)
    WHERE brief_note_uuid IS NOT NULL;

COMMENT ON COLUMN launch.launches.brief_note_uuid IS
    'Pre-minted uuid for this launch''s brief. It names the workspace.session_notes row (kind ''brief'') registration writes for every brief, and, for brief_mode ''note'', also the mailbox.messages row (message_uuid) the brief is mailed and pushed as; that message''s consumed_at is when the session acknowledged its brief. NULL when the launch had no prompt or no known session.';
COMMENT ON COLUMN launch.launches.brief_mode IS
    'How the brief reached claude: inline on the argv; file, staged to a 0600 file above the inline size limit (fallback); or note, mailed from mike under brief_note_uuid and pushed over Remote Control once the session is linked, with a typed pointer to the note as the fallback. NULL when there was no brief.';
