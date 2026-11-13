-- Session identity, part 3: spawn seeds identity so a spawned fleet session
-- is named, roled and briefed before its first turn, having called nothing
-- (operator follow-up, 2026-09-20 -- "ensure that ... registration easily
-- takes care of it").
--
-- `spawn_session` (MCP) and `mike spawn open` (CLI) record the identity they
-- were given on the launch.launches ledger row for the preassigned session
-- uuid (launch.launches.session_uuid, already indexed by
-- idx_launch_launches_session_uuid). When that session registers,
-- workspace registration looks up the most recent launch row for its uuid
-- with a non-null identity and applies it -- seeded, never overwriting a
-- value the session has since declared for itself (see
-- store/persistence/workspace/registration.rs).
--
-- One jsonb column, not four text columns, on a table already forty
-- columns wide: this is a one-shot handoff payload read once per
-- registration by session_uuid, not a queried or filtered dimension --
-- nothing sorts, groups or searches launches by name/role/instructions.
-- Shape: {"name": ..., "role": ..., "instructions": ..., "goal": ...,
-- "next_steps": ...}, every key optional. No index: the only lookup this
-- column needs is "the launch row for this session_uuid", already served
-- by idx_launch_launches_session_uuid.

ALTER TABLE launch.launches ADD COLUMN IF NOT EXISTS identity jsonb;

COMMENT ON COLUMN launch.launches.identity IS
    'Spawn-time identity handoff for the preassigned session_uuid: {"name", "role", "instructions", "goal", "next_steps"}, every key optional. Applied once by workspace registration (fetch_pending_launch_identity) and never re-applied over a value the session has since declared for itself. A one-shot payload keyed to session_uuid, not a queried dimension -- hence one jsonb column and no index beyond the existing session_uuid lookup.';
