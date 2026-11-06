-- Launch briefs live in mike, not in a file (operator ask, 2026-09-28: "can we
-- not actually build them into Mike as a session note inside Mike itself").
--
-- Until now a brief longer than the inline limit was written to
-- ~/.local/state/mike/briefs/<session>.md and the spawned session was typed a
-- pointer telling it to Read that file. The full text was already recorded
-- here as brief_text; the file was only the delivery channel.
--
-- Now, whenever the launch knows its session uuid, `mike spawn open` and
-- `spawn_session` mint a note uuid up front and record it as
-- brief_note_uuid. Workspace registration then materialises the brief as a
-- workspace.session_notes row with exactly that note_uuid (kind 'brief'),
-- the same seeded handoff launch identity already uses
-- (20260920000007_launch_identity.sql). Minting the uuid at launch, rather
-- than pre-creating the note, matters: a note needs a sessions.sessions row,
-- and pre-creating that row ahead of the session's first captured request
-- would skip the flow-admission columns capture sets on insert.
--
-- brief_mode 'note' is the new delivery mode: the argv carries a short ASCII
-- pointer naming the note, and the proxy's launch_brief stage attaches the
-- brief itself to the request on every turn. 'inline' launches get a
-- brief_note_uuid too, so every spawned prompt is findable as a note; 'file'
-- remains the fallback when the session uuid is unknown (--continue,
-- --resume --fork-session without --session-id) or the ledger is down.
--
-- No index: brief_note_uuid is only ever read together with session_uuid,
-- which idx_launch_launches_session_uuid already serves.

SET LOCAL lock_timeout = '5s';

ALTER TABLE launch.launches ADD COLUMN IF NOT EXISTS brief_note_uuid uuid;

ALTER TABLE launch.launches
    DROP CONSTRAINT IF EXISTS launches_brief_mode_check,
    ADD CONSTRAINT launches_brief_mode_check
        CHECK (brief_mode IN ('inline', 'file', 'note'));

COMMENT ON COLUMN launch.launches.brief_note_uuid IS
    'note_uuid of the workspace.session_notes row (kind ''brief'') this launch''s brief becomes. Minted at launch when session_uuid is known; the note itself is written by workspace registration or by the proxy''s launch_brief stage, whichever sees the session first (INSERT ... ON CONFLICT (note_uuid) DO NOTHING). NULL when the launch had no prompt or no known session.';
COMMENT ON COLUMN launch.launches.brief_mode IS
    'How the brief reached claude: inline on the argv; file, staged to a 0600 file above the inline size limit (fallback); or note, a pointer on the argv with the brief attached by the proxy from brief_text. NULL when there was no brief.';
