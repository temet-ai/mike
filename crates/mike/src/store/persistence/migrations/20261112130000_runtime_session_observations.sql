CREATE TABLE IF NOT EXISTS runtime.storage_accounting (
    kind text PRIMARY KEY CHECK (kind IN ('facts', 'diagnostics')),
    retained_rows bigint NOT NULL DEFAULT 0 CHECK (retained_rows >= 0),
    retained_bytes bigint NOT NULL DEFAULT 0 CHECK (retained_bytes >= 0)
);
INSERT INTO runtime.storage_accounting (kind) VALUES ('facts'), ('diagnostics')
ON CONFLICT (kind) DO NOTHING;

CREATE TABLE IF NOT EXISTS runtime.session_facts (
    session_id bigint PRIMARY KEY REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    runtime text NOT NULL CHECK (btrim(runtime) <> ''),
    source text NOT NULL,
    source_version text,
    fidelity text NOT NULL,
    completeness text NOT NULL,
    observed_at timestamptz NOT NULL,
    sequence bigint NOT NULL,
    lifecycle text CHECK (lifecycle IN ('active', 'idle', 'ended', 'failed')),
    lifecycle_at timestamptz,
    lifecycle_sequence bigint,
    usage_at timestamptz,
    usage_sequence bigint,
    control_at timestamptz,
    control_sequence bigint,
    requested_model_at timestamptz,
    observed_model_at timestamptz,
    requested_model text,
    observed_model text,
    control text CHECK (control IN ('managed', 'hook_boundary', 'pull_only')),
    usage_scope text,
    usage_basis text,
    input_tokens bigint CHECK (input_tokens >= 0),
    cached_input_tokens bigint CHECK (cached_input_tokens >= 0),
    output_tokens bigint CHECK (output_tokens >= 0),
    reasoning_output_tokens bigint CHECK (reasoning_output_tokens >= 0),
    total_tokens bigint CHECK (total_tokens >= 0),
    context_window bigint CHECK (context_window >= 0),
    retained_bytes bigint GENERATED ALWAYS AS (
        octet_length(runtime) + octet_length(source) + COALESCE(octet_length(source_version), 0)
        + octet_length(fidelity) + octet_length(completeness) + COALESCE(octet_length(lifecycle), 0)
        + COALESCE(octet_length(requested_model), 0) + COALESCE(octet_length(observed_model), 0)
        + COALESCE(octet_length(control), 0) + COALESCE(octet_length(usage_scope), 0)
        + COALESCE(octet_length(usage_basis), 0) + 512
    ) STORED
);

CREATE TABLE IF NOT EXISTS runtime.observation_receipts (
    session_id bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    observation_id uuid NOT NULL,
    fingerprint bytea NOT NULL,
    observed_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    retained_bytes bigint NOT NULL DEFAULT 128 CHECK (retained_bytes >= 0),
    PRIMARY KEY (session_id, observation_id)
);
CREATE INDEX IF NOT EXISTS idx_runtime_observation_receipts_expiry
    ON runtime.observation_receipts (expires_at);

CREATE TABLE IF NOT EXISTS runtime.model_calls (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    call_id text NOT NULL CHECK (btrim(call_id) <> ''),
    observed_at timestamptz NOT NULL,
    source text NOT NULL,
    source_version text,
    fidelity text NOT NULL,
    completeness text NOT NULL,
    requested_model text,
    observed_model text,
    status text,
    duration_ms bigint CHECK (duration_ms >= 0),
    http_status smallint,
    usage_scope text,
    usage_basis text,
    input_tokens bigint CHECK (input_tokens >= 0),
    cached_input_tokens bigint CHECK (cached_input_tokens >= 0),
    output_tokens bigint CHECK (output_tokens >= 0),
    reasoning_output_tokens bigint CHECK (reasoning_output_tokens >= 0),
    total_tokens bigint CHECK (total_tokens >= 0),
    context_window bigint CHECK (context_window >= 0),
    retained_bytes bigint GENERATED ALWAYS AS (
        octet_length(call_id) + octet_length(source) + COALESCE(octet_length(source_version), 0)
        + octet_length(fidelity) + octet_length(completeness) + COALESCE(octet_length(status), 0)
        + COALESCE(octet_length(requested_model), 0) + COALESCE(octet_length(observed_model), 0)
        + COALESCE(octet_length(usage_scope), 0) + COALESCE(octet_length(usage_basis), 0) + 512
    ) STORED,
    UNIQUE (session_id, call_id)
);
CREATE INDEX IF NOT EXISTS idx_runtime_model_calls_session_time
    ON runtime.model_calls (session_id, observed_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_runtime_model_calls_expiry
    ON runtime.model_calls USING brin (observed_at);

CREATE INDEX IF NOT EXISTS idx_runtime_model_calls_usage_source
    ON runtime.model_calls (source, session_id)
    WHERE input_tokens IS NOT NULL OR output_tokens IS NOT NULL;

CREATE TABLE IF NOT EXISTS runtime.session_items (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    item_id text NOT NULL CHECK (btrim(item_id) <> ''),
    turn_id text,
    kind text NOT NULL CHECK (btrim(kind) <> ''),
    name text,
    status text,
    success boolean,
    exit_code integer,
    source text NOT NULL,
    fidelity text NOT NULL,
    completeness text NOT NULL,
    first_seen_at timestamptz NOT NULL,
    observed_at timestamptz NOT NULL,
    retained_bytes bigint GENERATED ALWAYS AS (
        octet_length(item_id) + COALESCE(octet_length(turn_id), 0) + octet_length(kind)
        + COALESCE(octet_length(name), 0) + COALESCE(octet_length(status), 0)
        + octet_length(source) + octet_length(fidelity) + octet_length(completeness) + 512
    ) STORED,
    UNIQUE (session_id, item_id)
);
CREATE INDEX IF NOT EXISTS idx_runtime_session_items_time
    ON runtime.session_items (session_id, first_seen_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_runtime_session_items_expiry
    ON runtime.session_items USING brin (observed_at);

CREATE TABLE IF NOT EXISTS runtime.notification_targets (
    session_id bigint PRIMARY KEY REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    thread_id text NOT NULL,
    runtime_home text NOT NULL,
    verified_at timestamptz NOT NULL DEFAULT now(),
    retained_bytes bigint GENERATED ALWAYS AS (octet_length(thread_id) + octet_length(runtime_home) + 128) STORED
);
CREATE INDEX IF NOT EXISTS idx_runtime_notification_targets_expiry ON runtime.notification_targets (verified_at);

CREATE OR REPLACE FUNCTION runtime.account_observation_storage()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    row_delta bigint := 0;
    byte_delta bigint := 0;
BEGIN
    IF TG_OP <> 'DELETE' THEN
        row_delta := row_delta + 1;
        byte_delta := byte_delta + NEW.retained_bytes;
    END IF;
    IF TG_OP <> 'INSERT' THEN
        row_delta := row_delta - 1;
        byte_delta := byte_delta - OLD.retained_bytes;
    END IF;
    UPDATE runtime.storage_accounting
       SET retained_rows = retained_rows + row_delta,
           retained_bytes = retained_bytes + byte_delta
     WHERE kind = TG_ARGV[0];
    RETURN NULL;
END;
$$;

CREATE OR REPLACE TRIGGER account_runtime_session_facts
AFTER INSERT OR UPDATE OR DELETE ON runtime.session_facts
FOR EACH ROW EXECUTE FUNCTION runtime.account_observation_storage('facts');
CREATE OR REPLACE TRIGGER account_runtime_observation_receipts
AFTER INSERT OR UPDATE OR DELETE ON runtime.observation_receipts
FOR EACH ROW EXECUTE FUNCTION runtime.account_observation_storage('facts');
CREATE OR REPLACE TRIGGER account_runtime_model_calls
AFTER INSERT OR UPDATE OR DELETE ON runtime.model_calls
FOR EACH ROW EXECUTE FUNCTION runtime.account_observation_storage('facts');
CREATE OR REPLACE TRIGGER account_runtime_session_items
AFTER INSERT OR UPDATE OR DELETE ON runtime.session_items
FOR EACH ROW EXECUTE FUNCTION runtime.account_observation_storage('facts');
CREATE OR REPLACE TRIGGER account_runtime_notification_targets
AFTER INSERT OR UPDATE OR DELETE ON runtime.notification_targets
FOR EACH ROW EXECUTE FUNCTION runtime.account_observation_storage('facts');

CREATE TABLE IF NOT EXISTS mailbox.runtime_notifications (
    message_id bigint PRIMARY KEY REFERENCES mailbox.messages(id) ON DELETE CASCADE,
    state text NOT NULL CHECK (state IN ('attempted', 'queued', 'uncertain', 'failed')),
    attempted_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    detail text
);
