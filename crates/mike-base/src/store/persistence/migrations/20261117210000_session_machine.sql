-- sessions.sessions.machine_uuid: the machine whose mike captured the session
-- first. Several machines each run a mike that writes to this database, and a
-- session with no project location otherwise says nothing about where it runs.
-- No foreign key, like llm.worker_leases: a machine may stamp a session before
-- it has registered in projects.machines. Sessions bound to a project location
-- take that location's machine.
--
-- Every change is additive: a binary older than this migration keeps serving.

SET LOCAL lock_timeout = '5s';

ALTER TABLE sessions.sessions
    ADD COLUMN IF NOT EXISTS machine_uuid uuid;

COMMENT ON COLUMN sessions.sessions.machine_uuid IS 'Machine (projects.machines.machine_uuid, no foreign key) whose mike first captured the session. Stamped once when the session row is first written, and backfilled from the bound project location. NULL for a session captured before the column existed and never bound to a location.';

UPDATE sessions.sessions s
   SET machine_uuid = l.machine_uuid
  FROM projects.locations l
 WHERE s.location_uuid = l.location_uuid
   AND s.machine_uuid IS NULL;
