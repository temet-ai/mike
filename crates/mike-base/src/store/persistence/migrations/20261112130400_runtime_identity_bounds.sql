ALTER TABLE runtime.namespaces ADD COLUMN IF NOT EXISTS retained_bytes bigint GENERATED ALWAYS AS
    (octet_length(runtime_kind) + octet_length(durable_key) + octet_length(identity::text) + 256) STORED;
ALTER TABLE runtime.threads ADD COLUMN IF NOT EXISTS retained_bytes bigint GENERATED ALWAYS AS
    (octet_length(identity::text) * 2 + 512) STORED;

CREATE TABLE IF NOT EXISTS runtime.session_identities (
    session_id bigint PRIMARY KEY REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    retained_bytes bigint NOT NULL DEFAULT 65536 CHECK (retained_bytes = 65536)
);
COMMENT ON COLUMN runtime.session_identities.retained_bytes IS
    'Conservative reservation for one runtime session, its registration, identity metadata and project; adapter cwd is bounded to 4096 bytes.';

CREATE TABLE IF NOT EXISTS runtime.project_identities (
    project_id bigint PRIMARY KEY REFERENCES projects.projects(id) ON DELETE CASCADE,
    retained_bytes bigint NOT NULL DEFAULT 16384 CHECK (retained_bytes = 16384)
);

UPDATE runtime.storage_accounting SET retained_rows = totals.rows, retained_bytes = totals.bytes
FROM (SELECT count(*) AS rows, COALESCE(sum(retained_bytes), 0) AS bytes FROM (
    SELECT retained_bytes FROM runtime.namespaces UNION ALL
    SELECT retained_bytes FROM runtime.threads UNION ALL
    SELECT retained_bytes FROM runtime.session_identities UNION ALL
    SELECT retained_bytes FROM runtime.project_identities UNION ALL
    SELECT retained_bytes FROM runtime.session_facts UNION ALL
    SELECT retained_bytes FROM runtime.model_calls UNION ALL
    SELECT retained_bytes FROM runtime.observation_receipts UNION ALL
    SELECT retained_bytes FROM runtime.session_items UNION ALL
    SELECT retained_bytes FROM runtime.notification_targets
) retained) totals WHERE kind = 'facts';

CREATE OR REPLACE TRIGGER account_runtime_namespaces
AFTER INSERT OR UPDATE OR DELETE ON runtime.namespaces
FOR EACH ROW EXECUTE FUNCTION runtime.account_observation_storage('facts');
CREATE OR REPLACE TRIGGER account_runtime_threads
AFTER INSERT OR UPDATE OR DELETE ON runtime.threads
FOR EACH ROW EXECUTE FUNCTION runtime.account_observation_storage('facts');
CREATE OR REPLACE TRIGGER account_runtime_session_identities
AFTER INSERT OR UPDATE OR DELETE ON runtime.session_identities
FOR EACH ROW EXECUTE FUNCTION runtime.account_observation_storage('facts');

CREATE OR REPLACE TRIGGER account_runtime_project_identities
AFTER INSERT OR UPDATE OR DELETE ON runtime.project_identities
FOR EACH ROW EXECUTE FUNCTION runtime.account_observation_storage('facts');
