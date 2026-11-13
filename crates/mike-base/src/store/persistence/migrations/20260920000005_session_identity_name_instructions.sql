-- Session identity, part 1: a session can declare a name and the standing
-- brief it was launched under, alongside the `role` it already declares
-- (20260713140000_session_goals_role.sql). Operator ask (2026-09-20): fleet
-- sessions (observers, developers, coordinators) need a visible, settable
-- name, role, instructions and goal -- one row already carries goal/
-- next_steps/role per session, so identity extends that row rather than
-- growing a second 1:1 table.
--
-- `display_name` -- the operator- or self-declared handle for this session
-- ("observer-1", "rust-dev-alpha"). Free text, open set, same reasoning as
-- `role`: no enum, no uniqueness constraint. Two live sessions may declare
-- the same name; name resolution (mailbox `name:` addressing) reports
-- ambiguity rather than relying on a constraint to prevent it, exactly as
-- session-id-prefix resolution already does.
--
-- `instructions` -- the charter this session was given at launch or
-- registration: the standing brief it works under. Distinct from `goal`
-- (what it is trying to accomplish right now) and `next_steps` (what it
-- will do next) -- instructions is the brief, goal is the current objective
-- under that brief.
--
-- Both columns are nullable with no backfill: every existing session_goals
-- row predates identity and has neither a declared name nor a recorded
-- brief, and there is no source to backfill them from.

ALTER TABLE sessions.session_goals
    ADD COLUMN IF NOT EXISTS display_name text,
    ADD COLUMN IF NOT EXISTS instructions text;

COMMENT ON COLUMN sessions.session_goals.display_name IS
    'Free-text handle the session declares for itself (e.g. "observer-1", "rust-dev-alpha") -- open set, NOT unique. NULL when undeclared. Two live sessions may share a name; mailbox name: addressing (resolve_name_recipient) reports Ambiguous rather than relying on a uniqueness constraint. Case-insensitive lookups use idx_session_goals_display_name_lower.';

COMMENT ON COLUMN sessions.session_goals.instructions IS
    'The standing brief this session was given -- distinct from goal (the current objective) and next_steps (what it will do next). Free text, no enum. Set at registration (spawn-seeded or self-declared) or via set_session_goal. NULL when undeclared. Fenced as DATA, never instructions to the reader, wherever a peer session''s row is rendered (fleet roster, briefing).';

-- Name resolution (mailbox `name:` addressing) matches case-insensitively
-- against live registrations only, so the index is a partial functional
-- index on the lowercased value rather than a plain btree on display_name.
CREATE INDEX IF NOT EXISTS idx_session_goals_display_name_lower
    ON sessions.session_goals (lower(display_name))
    WHERE display_name IS NOT NULL;

-- The role fan-out path (resolve_role_recipients) and role: mailbox
-- addressing both match case-insensitively for the same reason name does;
-- this index gives that comparison something to use instead of the seq
-- scan the plain-equality query had before.
CREATE INDEX IF NOT EXISTS idx_session_goals_role_lower
    ON sessions.session_goals (lower(role))
    WHERE role IS NOT NULL;
