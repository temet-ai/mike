CREATE TABLE IF NOT EXISTS llm.worker_leases (
    role         text        PRIMARY KEY,
    machine_uuid uuid        NOT NULL,
    heartbeat_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE llm.worker_leases IS
    'One row per LLM worker role (llm.calls.role values): the machine that '
    'currently owns the role for this database. A proxy runs a role only while '
    'it holds the row. A holder renews heartbeat_at on each tick; another '
    'machine takes the row over once heartbeat_at is older than the timeout.';
COMMENT ON COLUMN llm.worker_leases.role IS
    'The llm.calls.role value of the worker the lease covers.';
COMMENT ON COLUMN llm.worker_leases.machine_uuid IS
    'machine_uuid from the owning machine''s machine.json.';
COMMENT ON COLUMN llm.worker_leases.heartbeat_at IS
    'Database clock time of the holder''s last claim or renewal.';

ALTER TABLE llm.calls
    ADD COLUMN IF NOT EXISTS machine_uuid uuid;

COMMENT ON COLUMN llm.calls.machine_uuid IS
    'machine_uuid of the proxy that made the call. NULL on calls recorded '
    'before this column existed and on calls from a proxy with no machine '
    'identity file.';
