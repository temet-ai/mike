-- Observed Codex sessions: what hooks and OpenTelemetry report about Codex sessions mike did not
-- launch. The original bytes of every hook delivery and OTLP export stay in runtime.events; these
-- tables are the identities and facts derived from them, keyed by Codex's own ids so hook,
-- telemetry, history and app-server evidence about one thread, turn or tool call land on one row.

CREATE TABLE runtime.codex_observed_threads (
    namespace_id uuid NOT NULL REFERENCES runtime.namespaces(id),
    thread_id text NOT NULL CHECK (btrim(thread_id) <> ''),
    root_thread_id text NOT NULL CHECK (btrim(root_thread_id) <> ''),
    parent_thread_id text NULL CHECK (parent_thread_id IS NULL OR btrim(parent_thread_id) <> ''),
    kind text NOT NULL CHECK (kind IN ('session', 'subagent')),
    agent_type text NULL,
    cwd text NULL,
    model text NULL,
    codex_version text NULL,
    originator text NULL,
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    ended_at timestamptz NULL,
    end_reason text NULL,
    sources text[] NOT NULL DEFAULT '{}',
    PRIMARY KEY (namespace_id, thread_id),
    CHECK ((kind = 'subagent') = (parent_thread_id IS NOT NULL))
);

CREATE INDEX idx_codex_observed_threads_root ON runtime.codex_observed_threads (namespace_id, root_thread_id);
CREATE INDEX idx_codex_observed_threads_last_seen ON runtime.codex_observed_threads (last_seen_at DESC);

CREATE TABLE runtime.codex_observed_turns (
    namespace_id uuid NOT NULL REFERENCES runtime.namespaces(id),
    turn_id text NOT NULL CHECK (btrim(turn_id) <> ''),
    thread_id text NOT NULL CHECK (btrim(thread_id) <> ''),
    prompt_submitted boolean NOT NULL DEFAULT false,
    model_requests_started integer NOT NULL DEFAULT 0 CHECK (model_requests_started >= 0),
    model_requests_finished integer NOT NULL DEFAULT 0 CHECK (model_requests_finished >= 0),
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    stopped_at timestamptz NULL,
    last_assistant_message text NULL,
    sources text[] NOT NULL DEFAULT '{}',
    PRIMARY KEY (namespace_id, turn_id)
);

CREATE INDEX idx_codex_observed_turns_thread ON runtime.codex_observed_turns (namespace_id, thread_id, first_seen_at);

CREATE TABLE runtime.codex_observed_tools (
    namespace_id uuid NOT NULL REFERENCES runtime.namespaces(id),
    call_id text NOT NULL CHECK (btrim(call_id) <> ''),
    thread_id text NULL,
    turn_id text NULL,
    kind text NOT NULL CHECK (kind IN ('shell', 'poll', 'code_mode', 'subagent_control', 'mcp', 'hosted', 'other')),
    hook_tool_name text NULL,
    telemetry_tool_name text NULL,
    telemetry_namespace text NULL,
    input json NULL,
    process_session text NULL,
    started_at timestamptz NULL,
    completed_at timestamptz NULL,
    duration_ms bigint NULL CHECK (duration_ms IS NULL OR duration_ms >= 0),
    success boolean NULL,
    exit_code integer NULL,
    hook_response_seen boolean NOT NULL DEFAULT false,
    telemetry_output_snippet boolean NOT NULL DEFAULT false,
    sources text[] NOT NULL DEFAULT '{}',
    PRIMARY KEY (namespace_id, call_id)
);

CREATE INDEX idx_codex_observed_tools_turn ON runtime.codex_observed_tools (namespace_id, turn_id) WHERE turn_id IS NOT NULL;
CREATE INDEX idx_codex_observed_tools_process ON runtime.codex_observed_tools (namespace_id, process_session) WHERE process_session IS NOT NULL;

CREATE TABLE runtime.codex_observed_requests (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    namespace_id uuid NOT NULL REFERENCES runtime.namespaces(id),
    ingestion_id uuid NOT NULL,
    ordinal integer NOT NULL CHECK (ordinal >= 0),
    source text NOT NULL CHECK (source IN ('hook', 'telemetry')),
    phase text NOT NULL CHECK (phase IN ('started', 'finished', 'attempt', 'response')),
    thread_id text NULL,
    turn_id text NULL,
    endpoint text NULL,
    attempt integer NULL,
    status text NULL,
    http_status integer NULL,
    success boolean NULL,
    error text NULL,
    duration_ms bigint NULL CHECK (duration_ms IS NULL OR duration_ms >= 0),
    usage json NULL,
    observed_at timestamptz NOT NULL,
    UNIQUE (namespace_id, ingestion_id, ordinal)
);

CREATE INDEX idx_codex_observed_requests_turn ON runtime.codex_observed_requests (namespace_id, turn_id) WHERE turn_id IS NOT NULL;
CREATE INDEX idx_codex_observed_requests_thread ON runtime.codex_observed_requests (namespace_id, thread_id, observed_at) WHERE thread_id IS NOT NULL;

CREATE TABLE runtime.codex_observed_hook_runs (
    namespace_id uuid NOT NULL REFERENCES runtime.namespaces(id),
    trace_id text NOT NULL,
    span_id text NOT NULL,
    event_name text NOT NULL,
    outcome text NOT NULL,
    execution_mode text NULL,
    timeout_sec integer NULL,
    started_at timestamptz NOT NULL,
    duration_ms bigint NOT NULL CHECK (duration_ms >= 0),
    PRIMARY KEY (namespace_id, trace_id, span_id)
);

CREATE INDEX idx_codex_observed_hook_runs_started ON runtime.codex_observed_hook_runs (namespace_id, started_at DESC);
