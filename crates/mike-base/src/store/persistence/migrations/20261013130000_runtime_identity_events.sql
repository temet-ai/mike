CREATE SCHEMA runtime;

CREATE TABLE runtime.namespaces (
    id uuid PRIMARY KEY,
    runtime_kind text NOT NULL CHECK (runtime_kind IN ('claude', 'codex')),
    durable_key text NOT NULL CHECK (btrim(durable_key) <> ''),
    identity jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (runtime_kind, durable_key),
    CHECK (id <> '00000000-0000-0000-0000-000000000000'::uuid),
    CHECK ((identity ->> 'id')::uuid = id),
    CHECK (identity ->> 'runtime' = runtime_kind),
    CHECK (identity ->> 'durable_key' = durable_key)
);

CREATE TABLE runtime.threads (
    session_id bigint PRIMARY KEY REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    namespace_id uuid NOT NULL REFERENCES runtime.namespaces(id),
    provider_thread_id text NOT NULL CHECK (btrim(provider_thread_id) <> ''),
    provider_session_id text,
    owning_seat_id bigint REFERENCES sessions.sessions(id) ON DELETE SET NULL,
    native_parent_namespace_id uuid REFERENCES runtime.namespaces(id),
    native_parent_thread_id text,
    fork_namespace_id uuid REFERENCES runtime.namespaces(id),
    fork_thread_id text,
    identity jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (namespace_id, provider_thread_id),
    CHECK ((native_parent_namespace_id IS NULL) = (native_parent_thread_id IS NULL)),
    CHECK ((fork_namespace_id IS NULL) = (fork_thread_id IS NULL)),
    CHECK (native_parent_thread_id IS NULL OR btrim(native_parent_thread_id) <> ''),
    CHECK (fork_thread_id IS NULL OR btrim(fork_thread_id) <> ''),
    CHECK (provider_session_id IS NULL OR btrim(provider_session_id) <> ''),
    CHECK (owning_seat_id IS NULL OR native_parent_thread_id IS NULL OR owning_seat_id <> session_id),
    CHECK (native_parent_namespace_id IS DISTINCT FROM namespace_id OR native_parent_thread_id <> provider_thread_id),
    CHECK (fork_namespace_id IS DISTINCT FROM namespace_id OR fork_thread_id <> provider_thread_id)
);

CREATE INDEX idx_runtime_threads_owning_seat ON runtime.threads (owning_seat_id)
    WHERE owning_seat_id IS NOT NULL;
CREATE INDEX idx_runtime_threads_native_parent ON runtime.threads
    (native_parent_namespace_id, native_parent_thread_id) WHERE native_parent_thread_id IS NOT NULL;
CREATE INDEX idx_runtime_threads_fork ON runtime.threads
    (fork_namespace_id, fork_thread_id) WHERE fork_thread_id IS NOT NULL;

CREATE TABLE runtime.events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    namespace_id uuid NOT NULL REFERENCES runtime.namespaces(id),
    ingestion_id uuid NOT NULL,
    provider_thread_id text,
    envelope json NOT NULL,
    raw_payload bytea NOT NULL,
    parsed_payload json,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (namespace_id, ingestion_id),
    CHECK (ingestion_id <> '00000000-0000-0000-0000-000000000000'::uuid),
    CHECK (provider_thread_id IS NULL OR btrim(provider_thread_id) <> '')
);

CREATE INDEX idx_runtime_events_thread_order ON runtime.events (namespace_id, provider_thread_id, id);
