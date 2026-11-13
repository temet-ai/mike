CREATE SCHEMA IF NOT EXISTS claude_proxy;

CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- ════════════════════════════════════════════════════════════════════
-- Domain-schema split (tables.sql).
--
-- Eleven Postgres schemas, one per domain. Lookup/reference tables live
-- WITH the domain they serve (no standalone lookup schema). Table names
-- drop the now-redundant prefix the schema already carries
-- (request_deltas -> requests.deltas).
--
--   projects        project metadata
--   sessions        session lifecycle
--   messages        deduped message-body store (+ role vocab)
--   system_prompts  deduped system-prompt store (+ block layout vocab)
--   rules           filter rules + volatile-line patterns
--   requests        request capture + per-request facts (+ request vocab)
--   responses       response capture + stream chunks (+ response vocab)
--   tools           tool invocations + results
--   agents          agent defs, dispatches, instances
--   overrides       prompt overrides + revisions
--   memory          operator turns, chunks, lessons
--
-- SCHEMA BLOCK ORDER below is dependency-topological, so every inline FK is
-- a BACK-reference. Two FKs cannot be inline and live in constraints.sql:
--   1. requests.requests.agent_instance_id -> agents.instances  (true cycle:
--      requests -> agent_instances -> agent_dispatches -> tool_uses -> requests)
--   2. requests.system_prompts.override_id -> overrides.overrides
--      (forward: requests block precedes overrides block)
-- requests.requests.agent_instance_id and requests.system_prompts.override_id
-- are therefore declared as plain bigint columns here.
--
-- The CREATE SCHEMA statements may ultimately move to the schema/extensions
-- file (squash 1/5); kept here so tables.sql reviews stand alone.
-- ════════════════════════════════════════════════════════════════════

CREATE SCHEMA IF NOT EXISTS projects;
CREATE SCHEMA IF NOT EXISTS sessions;
CREATE SCHEMA IF NOT EXISTS messages;
CREATE SCHEMA IF NOT EXISTS system_prompts;
CREATE SCHEMA IF NOT EXISTS rules;
CREATE SCHEMA IF NOT EXISTS requests;
CREATE SCHEMA IF NOT EXISTS responses;
CREATE SCHEMA IF NOT EXISTS tools;
CREATE SCHEMA IF NOT EXISTS agents;
CREATE SCHEMA IF NOT EXISTS overrides;
CREATE SCHEMA IF NOT EXISTS memory;

-- ════════════════════════════════════════════════════════════════════
-- projects
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE projects.projects (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    project_path text NOT NULL,
    display_name text NOT NULL,
    first_seen timestamptz DEFAULT now() NOT NULL,
    last_seen timestamptz DEFAULT now() NOT NULL,
    pinned boolean DEFAULT false NOT NULL,
    ttl_override interval,
    pin_created_at timestamptz,
    ttl_override_set_at timestamptz
);
CREATE INDEX idx_projects_last_seen ON projects.projects USING btree (last_seen DESC);
CREATE INDEX idx_projects_pinned ON projects.projects USING btree (id) WHERE (pinned = true);

-- ════════════════════════════════════════════════════════════════════
-- sessions
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE sessions.sessions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_uuid uuid NOT NULL UNIQUE,
    project_id bigint NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    title text,
    pinned boolean DEFAULT false NOT NULL,
    ttl_override interval,
    first_seen timestamptz DEFAULT now() NOT NULL,
    last_seen timestamptz DEFAULT now() NOT NULL,
    last_status text,
    pin_created_at timestamptz,
    ttl_override_set_at timestamptz
);
CREATE INDEX brin_sessions_last_seen ON sessions.sessions USING brin (last_seen) WITH (pages_per_range='32');
CREATE INDEX idx_sessions_pinned ON sessions.sessions USING btree (id) WHERE (pinned = true);
CREATE INDEX idx_sessions_project_last_seen ON sessions.sessions USING btree (project_id, last_seen DESC);
CREATE INDEX idx_sessions_title_trgm ON sessions.sessions USING gin (title gin_trgm_ops) WHERE (title IS NOT NULL);

-- ════════════════════════════════════════════════════════════════════
-- messages — deduped message-body store + role vocab
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE messages.roles (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO messages.roles (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'user'),
    (2, 'assistant'),
    (3, 'system');

CREATE TABLE messages.messages (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    message_sha256 text NOT NULL,
    body jsonb NOT NULL,
    byte_size integer NOT NULL,
    role_id bigint NOT NULL REFERENCES messages.roles(id),
    first_seen timestamptz DEFAULT now() NOT NULL,
    last_seen timestamptz DEFAULT now() NOT NULL
);
CREATE INDEX idx_messages_last_seen ON messages.messages USING btree (last_seen DESC);

-- ════════════════════════════════════════════════════════════════════
-- system_prompts — deduped system-prompt store + block-layout vocab
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE system_prompts.blocks (
    -- Natural positional key: this IS the index into the wire system[] array,
    -- not a surrogate. Deliberately diverges from the id-IDENTITY lookup
    -- convention because the value is positional — the classifier strip logic
    -- indexes system[N] by it directly; a surrogate id would force an
    -- id→index translation in Rust.
    block_index integer PRIMARY KEY,
    code        text NOT NULL UNIQUE,
    description text NOT NULL
);
INSERT INTO system_prompts.blocks (block_index, code, description) VALUES
    (0, 'billing_header',    'Harness-injected per-process billing line (cc_version, cch…). Volatile every process.'),
    (1, 'identity_preamble', 'System metadata / identity preamble; may carry cache_control.'),
    (2, 'role_body',         'Agent identity prompt (10–80 KB). Hashed for prompt-class identity.');

CREATE TABLE system_prompts.system_prompts (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    prompt_sha256 text NOT NULL,
    blocks jsonb NOT NULL,
    first_seen_at timestamptz DEFAULT now() NOT NULL,
    last_seen_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT prompt_sha256_is_64_hex CHECK ((prompt_sha256 ~ '^[0-9a-f]{64}$'::text))
);
CREATE INDEX idx_system_prompts_sha256 ON system_prompts.system_prompts USING btree (prompt_sha256);

-- ════════════════════════════════════════════════════════════════════
-- rules — filter rules + volatile-line patterns
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE rules.kinds (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO rules.kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'text_substring'),
    (2, 'xml_tag');

CREATE TABLE rules.actions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO rules.actions (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'drop'),
    (2, 'replace'),
    (3, 'append');

CREATE TABLE rules.sources (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO rules.sources (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'toml'),
    (2, 'ui');

CREATE TABLE rules.rules (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uuid uuid NOT NULL,
    kind_id bigint NOT NULL REFERENCES rules.kinds(id),
    value text NOT NULL,
    action_id bigint NOT NULL REFERENCES rules.actions(id),
    text text,
    enabled boolean DEFAULT true NOT NULL,
    source_id bigint NOT NULL REFERENCES rules.sources(id),
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL
);

CREATE TABLE rules.volatile_line_patterns (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uuid uuid DEFAULT gen_random_uuid() NOT NULL,
    name text NOT NULL,
    pattern text NOT NULL,
    applies_to_block_index integer NOT NULL REFERENCES system_prompts.blocks(block_index),
    enabled boolean DEFAULT true NOT NULL,
    notes text,
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL
);
CREATE INDEX idx_volatile_line_patterns_applies_to_block_index ON rules.volatile_line_patterns USING btree (applies_to_block_index);

-- ════════════════════════════════════════════════════════════════════
-- requests — request capture + per-request facts + request vocab
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE requests.methods (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO requests.methods (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'GET'),
    (2, 'POST'),
    (3, 'PUT'),
    (4, 'PATCH'),
    (5, 'DELETE'),
    (6, 'HEAD'),
    (7, 'OPTIONS'),
    (8, 'TRACE'),
    (9, 'CONNECT');

CREATE TABLE requests.statuses (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO requests.statuses (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'pending'),
    (2, 'streaming'),
    (3, 'complete'),
    (4, 'error');

CREATE TABLE requests.turn_types (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO requests.turn_types (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'user_prompt'),
    (2, 'tool_loop'),
    (3, 'continuation');

CREATE TABLE requests.turn_origins (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO requests.turn_origins (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'primary'),
    (2, 'title_sidecar'),
    (3, 'subagent'),
    (4, 'advisor_sidecar'),
    (5, 'quota_probe'),
    (6, 'primary_first');

CREATE TABLE requests.delta_change_kinds (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO requests.delta_change_kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'added'),
    (2, 'removed');

CREATE TABLE requests.delta_removed_reasons (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO requests.delta_removed_reasons (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'compaction'),
    (2, 'tail_rebuild'),
    (3, 'unknown');

CREATE TABLE requests.requests (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    project_id bigint NOT NULL REFERENCES projects.projects(id),
    method_id bigint NOT NULL REFERENCES requests.methods(id),
    path text NOT NULL,
    model text,
    is_streaming boolean NOT NULL,
    message_count integer NOT NULL,
    is_tool_loop boolean NOT NULL,
    status_id bigint NOT NULL REFERENCES requests.statuses(id),
    request_headers jsonb,
    params jsonb NOT NULL,
    last_user_message text NOT NULL,
    human_prompt text NOT NULL,
    turn_type_id bigint NOT NULL REFERENCES requests.turn_types(id),
    turn_origin_id bigint NOT NULL REFERENCES requests.turn_origins(id),
    subagent_persona text,
    created_at timestamptz DEFAULT now() NOT NULL,
    completed_at timestamptz,
    persisted_at timestamptz DEFAULT now() NOT NULL,
    request_body bytea,
    filtered_request_body bytea,
    intercepted boolean DEFAULT false NOT NULL,
    correction_request_id bigint REFERENCES requests.requests(id) ON DELETE SET NULL,
    -- FK -> agents.instances(id) is the requests↔agent_instances cycle break;
    -- declared plain here, constrained in constraints.sql (ON DELETE SET NULL).
    agent_instance_id bigint,
    -- FK -> memory.operator_turns(id): forward edge (requests block precedes the memory
    -- block), declared plain here and constrained in constraints.sql (ON DELETE SET NULL).
    -- NULL for requests that precede any user-prompt in their session (title-sidecars,
    -- quota-probes, pre-first-prompt captures). Canonical turn-membership fact: per-turn
    -- aggregates join r.operator_turn_id = ot.id instead of a BETWEEN range scan + session
    -- re-check. Populated by the Rust turn-classifier as it opens/extends the turn (no DB
    -- trigger). start_request_id/end_request_id on operator_turns remain as convenience
    -- bounds; this column is the source of truth for membership.
    operator_turn_id bigint
);

CREATE INDEX brin_requests_created_at ON requests.requests USING brin (created_at) WITH (pages_per_range='32');
CREATE INDEX idx_requests_session_id ON requests.requests USING btree (session_id);
CREATE INDEX idx_requests_project_created_desc ON requests.requests USING btree (project_id, created_at DESC);
CREATE INDEX idx_requests_correction_request_id ON requests.requests USING btree (correction_request_id) WHERE (correction_request_id IS NOT NULL);
CREATE INDEX requests_agent_instance_id_idx ON requests.requests USING btree (agent_instance_id) WHERE (agent_instance_id IS NOT NULL);
CREATE INDEX idx_requests_inflight_session ON requests.requests USING btree (session_id) WHERE (status_id = ANY (ARRAY[1, 2]));
CREATE INDEX idx_requests_model_created_at ON requests.requests USING btree (model, created_at DESC) WHERE (model IS NOT NULL);
CREATE INDEX idx_requests_session_completed ON requests.requests USING btree (session_id, completed_at) WHERE (completed_at IS NOT NULL);
CREATE INDEX idx_requests_status_created_at ON requests.requests USING btree (status_id, created_at DESC);
CREATE INDEX idx_requests_turn_origin_created_at ON requests.requests USING btree (turn_origin_id, created_at DESC);
CREATE INDEX idx_requests_operator_turn_id ON requests.requests USING btree (operator_turn_id) WHERE (operator_turn_id IS NOT NULL);

CREATE TABLE requests.messages (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    message_id bigint NOT NULL REFERENCES messages.messages(id),
    ord integer NOT NULL,
    CONSTRAINT request_messages_ord_nonneg CHECK ((ord >= 0))
);

COMMENT ON TABLE requests.messages IS 'Ordered FK list per request; one row per (request, ord) pair. message_id -> messages.messages(id); the content sha is sourced via JOIN messages.messages m ON m.id = message_id (no denormalised copy).';

CREATE INDEX idx_request_messages_request_id ON requests.messages USING btree (request_id);
CREATE INDEX idx_request_messages_message_id ON requests.messages USING btree (message_id);

CREATE TABLE requests.deltas (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    predecessor_request_id bigint REFERENCES requests.requests(id) ON DELETE SET NULL,
    session_id bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    message_id bigint NOT NULL REFERENCES messages.messages(id),
    change_kind_id bigint NOT NULL REFERENCES requests.delta_change_kinds(id),
    ord integer NOT NULL,
    role_id bigint NOT NULL REFERENCES messages.roles(id),
    byte_size integer NOT NULL,
    body jsonb NOT NULL,
    removed_reason_id bigint REFERENCES requests.delta_removed_reasons(id),
    computed_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT request_deltas_byte_size_nonneg CHECK ((byte_size >= 0)),
    CONSTRAINT request_deltas_ord_nonneg CHECK ((ord >= 0)),
    CONSTRAINT request_deltas_reason_only_on_removed CHECK ((((change_kind_id = 1) AND (removed_reason_id IS NULL)) OR ((change_kind_id = 2) AND (removed_reason_id IS NOT NULL))))
);

COMMENT ON TABLE requests.deltas IS 'Per-request immutable delta fact table. One row per message change (added or removed). Carries the full canonical message content inline so a single SELECT shows exactly what changed. Write-once at snapshot time.';

CREATE INDEX idx_request_deltas_request_id ON requests.deltas USING btree (request_id);
CREATE INDEX idx_request_deltas_session_id ON requests.deltas USING btree (session_id);
CREATE INDEX idx_request_deltas_message_id ON requests.deltas USING btree (message_id);
CREATE INDEX idx_request_deltas_role ON requests.deltas USING btree (role_id);
CREATE INDEX idx_request_deltas_removed_reason ON requests.deltas USING btree (removed_reason_id) WHERE (removed_reason_id IS NOT NULL);

CREATE TABLE requests.diagnostics (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    category text NOT NULL,
    analyzer text NOT NULL,
    advice text NOT NULL,
    matched_text text NOT NULL,
    match_range jsonb,
    other_hits jsonb DEFAULT '[]'::jsonb NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL
);

COMMENT ON TABLE requests.diagnostics IS 'One row per analyzer finding attached to a captured request. Inspection-fidelity: every column carries full analyzer-emitted bytes (no truncation, no preview/summary derivatives).';

CREATE INDEX idx_request_diagnostics_request_id ON requests.diagnostics USING btree (request_id);
CREATE INDEX idx_request_diagnostics_analyzer_created_at ON requests.diagnostics USING btree (analyzer, created_at DESC);
CREATE INDEX idx_request_diagnostics_category_created_at ON requests.diagnostics USING btree (category, created_at DESC);
-- Time-range index for aggregate_diagnostic_stats (WHERE created_at > NOW()-'24h' GROUP BY …)
-- and for unfiltered list_diagnostic_findings ORDER BY created_at DESC LIMIT n.
-- The composite (category/analyzer, created_at) indexes above serve filtered queries;
-- this index serves the unfiltered time-window scan.
CREATE INDEX idx_request_diagnostics_created_at ON requests.diagnostics USING btree (created_at DESC);

CREATE TABLE requests.tools (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    ord integer NOT NULL,
    tool_name text NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL
);

COMMENT ON TABLE requests.tools IS 'One row per tool DECLARED in a captured request body (body.tools[].name), preserving declaration order via ord (0-based). Semantically distinct from tools.uses (tools INVOKED). Inspection-fidelity: tool_name is the FULL declared name, no truncation.';

CREATE INDEX idx_request_tools_request ON requests.tools USING btree (request_id);
CREATE INDEX idx_request_tools_name ON requests.tools USING btree (tool_name);

CREATE TABLE requests.errors (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    message text NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL
);

COMMENT ON TABLE requests.errors IS 'One row per request that terminated in error (status_id = 4). `message` is the FULL error text (the former `Error: <message>` payload), never truncated. Absent for non-error requests. request_id -> requests.requests(id) ON DELETE CASCADE.';

CREATE INDEX idx_request_errors_request_id ON requests.errors USING btree (request_id);

CREATE TABLE requests.system_prompts (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    prompt_sha256 text,
    -- FK -> overrides.overrides(id) is a forward edge (requests block
    -- precedes overrides block); declared plain here, constrained in
    -- constraints.sql.
    override_id bigint,
    system_prompt_id bigint REFERENCES system_prompts.system_prompts(id),
    override_applied boolean DEFAULT false NOT NULL,
    unknown_blocks jsonb,
    filter_modified boolean DEFAULT false NOT NULL
);
CREATE INDEX idx_request_system_prompts_override_id ON requests.system_prompts USING btree (override_id);
CREATE INDEX idx_request_system_prompts_sysprompt_id ON requests.system_prompts USING btree (system_prompt_id);
CREATE INDEX idx_rsp_prompt_sha256 ON requests.system_prompts USING btree (prompt_sha256) WHERE (prompt_sha256 IS NOT NULL);

CREATE TABLE requests.system_prompt_firing_rules (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_system_prompt_id bigint NOT NULL REFERENCES requests.system_prompts(id) ON DELETE CASCADE,
    filter_rule_id bigint NOT NULL REFERENCES rules.rules(id) ON DELETE RESTRICT,
    ord integer NOT NULL,
    CONSTRAINT rsp_firing_rules_ord_nonneg CHECK ((ord >= 0))
);

COMMENT ON TABLE requests.system_prompt_firing_rules IS 'Ordered FK junction: which rules.rules fired (in fire order) for a request''s system-prompt observation. ord is the 0-based fire order. filter_rule_id -> rules.rules(id); request_system_prompt_id -> requests.system_prompts(id), both ON DELETE CASCADE.';

CREATE INDEX idx_rsp_firing_rules_filter_rule_id ON requests.system_prompt_firing_rules USING btree (filter_rule_id);

-- ════════════════════════════════════════════════════════════════════
-- responses — response capture + stream chunks + response vocab
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE responses.statuses (
    id smallint PRIMARY KEY,
    reason text NOT NULL,
    CONSTRAINT response_statuses_code_range CHECK (((id >= 100) AND (id <= 599)))
);
INSERT INTO responses.statuses (id, reason) VALUES
    (100, 'Continue'),
    (101, 'Switching Protocols'),
    (102, 'Processing'),
    (103, 'Early Hints'),
    (200, 'OK'),
    (201, 'Created'),
    (202, 'Accepted'),
    (203, 'Non-Authoritative Information'),
    (204, 'No Content'),
    (205, 'Reset Content'),
    (206, 'Partial Content'),
    (207, 'Multi-Status'),
    (208, 'Already Reported'),
    (226, 'IM Used'),
    (300, 'Multiple Choices'),
    (301, 'Moved Permanently'),
    (302, 'Found'),
    (303, 'See Other'),
    (304, 'Not Modified'),
    (305, 'Use Proxy'),
    (307, 'Temporary Redirect'),
    (308, 'Permanent Redirect'),
    (400, 'Bad Request'),
    (401, 'Unauthorized'),
    (402, 'Payment Required'),
    (403, 'Forbidden'),
    (404, 'Not Found'),
    (405, 'Method Not Allowed'),
    (406, 'Not Acceptable'),
    (407, 'Proxy Authentication Required'),
    (408, 'Request Timeout'),
    (409, 'Conflict'),
    (410, 'Gone'),
    (411, 'Length Required'),
    (412, 'Precondition Failed'),
    (413, 'Content Too Large'),
    (414, 'URI Too Long'),
    (415, 'Unsupported Media Type'),
    (416, 'Range Not Satisfiable'),
    (417, 'Expectation Failed'),
    (418, 'I''m a teapot'),
    (421, 'Misdirected Request'),
    (422, 'Unprocessable Content'),
    (423, 'Locked'),
    (424, 'Failed Dependency'),
    (425, 'Too Early'),
    (426, 'Upgrade Required'),
    (428, 'Precondition Required'),
    (429, 'Too Many Requests'),
    (431, 'Request Header Fields Too Large'),
    (451, 'Unavailable For Legal Reasons'),
    (500, 'Internal Server Error'),
    (501, 'Not Implemented'),
    (502, 'Bad Gateway'),
    (503, 'Service Unavailable'),
    (504, 'Gateway Timeout'),
    (505, 'HTTP Version Not Supported'),
    (506, 'Variant Also Negotiates'),
    (507, 'Insufficient Storage'),
    (508, 'Loop Detected'),
    (510, 'Not Extended'),
    (511, 'Network Authentication Required'),
    (520, 'Web Server Returned an Unknown Error'),
    (521, 'Web Server Is Down'),
    (522, 'Connection Timed Out'),
    (523, 'Origin Is Unreachable'),
    (524, 'A Timeout Occurred'),
    (525, 'SSL Handshake Failed'),
    (526, 'Invalid SSL Certificate'),
    (527, 'Railgun Error'),
    (529, 'Site Overloaded');

CREATE TABLE responses.stop_reasons (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO responses.stop_reasons (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'end_turn'),
    (2, 'max_tokens'),
    (3, 'stop_sequence'),
    (4, 'tool_use'),
    (5, 'pause_turn'),
    (6, 'error'),
    (7, 'refusal');

CREATE TABLE responses.stream_delta_types (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO responses.stream_delta_types (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'text'),
    (2, 'thinking'),
    (3, 'input_json');

CREATE TABLE responses.responses (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    response_status smallint NOT NULL REFERENCES responses.statuses(id),
    response_headers jsonb DEFAULT '[]'::jsonb NOT NULL,
    response_body bytea DEFAULT '\x'::bytea NOT NULL,
    upstream_request_id text,
    upstream_message_id text,
    token_usage jsonb DEFAULT '{}'::jsonb NOT NULL,
    -- Typed token counts hoisted out of token_usage at WRITE time (GENERATED STORED), so the
    -- per-turn token sums are plain sum(col) instead of a read-time regex+cast in correlated
    -- subqueries. token_usage stays the FULL JSONB source of truth; the `~ '^[0-9]+$'` guard
    -- yields NULL for absent/non-numeric keys (sum() skips NULLs). Must live here, beside their
    -- source column — a GENERATED column can only reference its own row.
    input_tokens bigint GENERATED ALWAYS AS (
        CASE WHEN (token_usage ->> 'input_tokens') ~ '^[0-9]+$'
             THEN (token_usage ->> 'input_tokens')::bigint END) STORED,
    output_tokens bigint GENERATED ALWAYS AS (
        CASE WHEN (token_usage ->> 'output_tokens') ~ '^[0-9]+$'
             THEN (token_usage ->> 'output_tokens')::bigint END) STORED,
    cache_creation_input_tokens bigint GENERATED ALWAYS AS (
        CASE WHEN (token_usage ->> 'cache_creation_input_tokens') ~ '^[0-9]+$'
             THEN (token_usage ->> 'cache_creation_input_tokens')::bigint END) STORED,
    cache_read_input_tokens bigint GENERATED ALWAYS AS (
        CASE WHEN (token_usage ->> 'cache_read_input_tokens') ~ '^[0-9]+$'
             THEN (token_usage ->> 'cache_read_input_tokens')::bigint END) STORED,
    stop_reason_id bigint REFERENCES responses.stop_reasons(id),
    stop_details jsonb,
    completed_at timestamptz NOT NULL,
    persisted_at timestamptz DEFAULT now() NOT NULL
);

COMMENT ON TABLE responses.responses IS 'Response-phase data for each completed request. One row per terminal request — absent = in-flight or error-before-response. Write-once at snapshot_terminal time.';
COMMENT ON COLUMN responses.responses.upstream_request_id IS 'Opaque external identifier from Anthropic''s `request-id` response header (e.g. ''req_…''). Unique per upstream HTTP request, minted by the API; stored verbatim as text. Not a closed domain — never normalised to a lookup FK.';
COMMENT ON COLUMN responses.responses.upstream_message_id IS 'Opaque external identifier from the Anthropic response body''s message `id` (e.g. ''msg_…''). Unique per response, minted by the API; stored verbatim as text. Not a closed domain — never normalised to a lookup FK.';
COMMENT ON COLUMN responses.responses.stop_details IS 'Refusal-category object from refusal responses (stop_reason = ''refusal''). NULL for all other terminations. Captured verbatim from the wire.';

CREATE INDEX idx_request_responses_request_id ON responses.responses USING btree (request_id);
CREATE INDEX idx_request_responses_completed_at ON responses.responses USING brin (completed_at);
CREATE INDEX idx_request_responses_stop_reason ON responses.responses USING btree (stop_reason_id) WHERE (stop_reason_id IS NOT NULL);
CREATE INDEX idx_request_responses_upstream_request_id ON responses.responses USING btree (upstream_request_id) WHERE (upstream_request_id IS NOT NULL);

CREATE TABLE responses.errors (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    response_id bigint NOT NULL REFERENCES responses.responses(id) ON DELETE CASCADE,
    message text NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL
);

COMMENT ON TABLE responses.errors IS 'One row per response-phase error (upstream/mid-stream failure on a captured response). `message` is the FULL error text, never truncated. response_id -> responses.responses(id) ON DELETE CASCADE. Mirrors requests.errors.';

CREATE INDEX idx_response_errors_response_id ON responses.errors USING btree (response_id);

CREATE TABLE responses.stream_chunks (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    session_id bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    seq_ord integer NOT NULL,
    chunk_content text NOT NULL,
    chunk_byte_size integer NOT NULL,
    chunk_sha256 text NOT NULL,
    delta_type_id bigint NOT NULL REFERENCES responses.stream_delta_types(id),
    arrived_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT request_text_deltas_chunk_byte_size_nonneg CHECK ((chunk_byte_size >= 0)),
    CONSTRAINT request_text_deltas_chunk_sha256_is_64_hex CHECK ((chunk_sha256 ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT request_text_deltas_seq_ord_nonneg CHECK ((seq_ord >= 0))
);

COMMENT ON TABLE responses.stream_chunks IS 'Per-request SSE streaming content chunks. One row per content_block_delta event; delta_type_id discriminates text / thinking / input_json. Write-once at capture time; reconstructed via string_agg.';
COMMENT ON COLUMN responses.stream_chunks.chunk_content IS 'Raw content of the delta: plain text for text/thinking, partial JSON string for input_json.';

CREATE INDEX idx_response_stream_chunks_request_id ON responses.stream_chunks USING btree (request_id);
CREATE INDEX idx_response_stream_chunks_session_id ON responses.stream_chunks USING btree (session_id);
CREATE INDEX idx_response_stream_chunks_request_delta_type ON responses.stream_chunks USING btree (request_id, delta_type_id, seq_ord);
CREATE INDEX idx_response_stream_chunks_arrived_at ON responses.stream_chunks USING brin (arrived_at);

-- ════════════════════════════════════════════════════════════════════
-- tools — tool invocations + results
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE tools.uses (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    wire_tool_use_id text NOT NULL,
    request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    ord integer NOT NULL,
    tool_name text NOT NULL,
    input jsonb,
    -- Resolved at CAPTURE time by the Rust tool_name->path-field registry (NOT a generated
    -- column — a closed DDL expression cannot evolve per-tool without a migration nor reach
    -- non-top-level keys; NOT a trigger). file_path = the touched path, NULL for
    -- non-path-bearing tools; operation = an OPEN label (read/write/edit/…), NULL when the
    -- registry has none. No CHECK enum on operation and no tool-name list anywhere in the
    -- schema: tool_name carries the truth, and the registry is an open allow-list grown by
    -- adding an entry — never a closed set frozen in SQL. Unclassified tools keep their full
    -- `input`, so a later registry addition + backfill lights them up retroactively.
    file_path text,
    operation text,
    created_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT uq_tool_uses_wire_tool_use_id UNIQUE (wire_tool_use_id),
    CONSTRAINT uq_tool_uses_request_ord UNIQUE (request_id, ord)
);

COMMENT ON TABLE tools.uses IS 'One row per tool_use block emitted by the model (captured from the response SSE stream, write-as-you-go). `input` is the FULL wire payload (no truncation). Surrogate `id` is the PK and the FK target for children (tools.results, agents.dispatches); `wire_tool_use_id` is the natural key. FK request_id -> requests.requests(id) ON DELETE CASCADE.';
COMMENT ON COLUMN tools.uses.wire_tool_use_id IS 'The provider-assigned tool_use block id verbatim from the wire (Anthropic `toolu_...`; the tool_use block''s `id` field). Natural key: UNIQUE, deduped globally via ON CONFLICT DO NOTHING — because every follow-up request replays the whole history, the same id is carried by many request bodies, so the FIRST sighting (the emitting response) wins and request_id anchors that emitter. Incoming tool_result blocks reference this value (their `tool_use_id`) and are resolved to the surrogate `id` by a write-time join. Provider-agnostic by design: the column holds whatever opaque id the provider emits.';
COMMENT ON COLUMN tools.uses.ord IS 'Zero-based position of this tool_use block within its emitting request. Ordering for sequence/timeline analysis is (requests.created_at, ord).';
COMMENT ON COLUMN tools.uses.id IS 'Surrogate primary key (DB-assigned identity). Stable internal handle that children (tools.results, agents.dispatches) FK to, decoupled from the provider''s opaque wire_tool_use_id.';
COMMENT ON COLUMN tools.uses.request_id IS 'FK -> requests.requests(id) of the request whose RESPONSE emitted this tool_use block (the emitter, not a later request that merely replayed it — see wire_tool_use_id''s first-sighting dedup). ON DELETE CASCADE.';
COMMENT ON COLUMN tools.uses.tool_name IS 'Name of the invoked tool verbatim from the wire (e.g. Bash, Read, Edit, Write, Agent, mcp__*). FULL name, no truncation. Primary grouping key for analysis; indexed.';
COMMENT ON COLUMN tools.uses.input IS 'The FULL tool_use input payload as JSONB, verbatim from the wire (no truncation): the tool''s arguments (e.g. Bash {command, description}, Read/Edit/Write {file_path, ...}). Source of truth for command/argument mining; normalization/templating belongs in the analysis layer, not here. NULL only when the model emitted an empty input. The touched path is hoisted to the capture-time-resolved file_path column (partial-indexed); a GIN trigram index covers input->>''command'' (Bash).';
COMMENT ON COLUMN tools.uses.created_at IS 'Capture timestamp (write-as-you-go, set when the tool_use block was first seen on the response stream). Secondary key after request ordering for timeline reconstruction.';

CREATE INDEX idx_tool_uses_request_id ON tools.uses USING btree (request_id);
CREATE INDEX idx_tool_uses_name ON tools.uses USING btree (tool_name);
CREATE INDEX idx_tool_uses_file_path ON tools.uses USING btree (file_path) WHERE (file_path IS NOT NULL);
CREATE INDEX idx_tool_uses_bash_command_trgm ON tools.uses USING gin (((input ->> 'command'::text)) gin_trgm_ops) WHERE (tool_name = 'Bash'::text);

CREATE TABLE tools.results (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    tool_use_id bigint NOT NULL REFERENCES tools.uses(id) ON DELETE CASCADE,
    ord integer NOT NULL,
    is_error boolean NOT NULL,
    content jsonb NOT NULL,
    content_sha256 text NOT NULL,
    content_bytes bigint NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL
);

COMMENT ON TABLE tools.results IS 'One row per tool_result block carried by a follow-up request. tool_use_id is the surrogate bigint FK -> tools.uses(id) ON DELETE CASCADE, resolved at write time by joining the wire id (the block''s `tool_use_id`) against tools.uses.wire_tool_use_id; a result whose tool_use was never captured is silently dropped by that INNER JOIN. `content` is FULL bytes (string OR array of typed blocks).';
COMMENT ON COLUMN tools.results.is_error IS 'The tool_result block''s `is_error` flag; absent on the wire is captured as false (Anthropic treats absent as not-an-error).';
COMMENT ON COLUMN tools.results.content_bytes IS 'Byte length of the full result content. Together with is_error it separates the three outcomes mass-analysis needs: error (is_error=true), success-with-output (is_error=false, content_bytes large), and ran-but-empty (is_error=false, content_bytes~0, e.g. a grep that matched nothing) — the last is a negative signal for command promotability, distinct from both.';
COMMENT ON COLUMN tools.results.id IS 'Surrogate primary key (DB-assigned identity).';
COMMENT ON COLUMN tools.results.request_id IS 'FK -> requests.requests(id) ON DELETE CASCADE of the follow-up request whose BODY carried this tool_result block. Note: this is the request that returned the result to the model, NOT the request that emitted the originating tool_use (that is tools.uses.request_id, reachable via tool_use_id).';
COMMENT ON COLUMN tools.results.tool_use_id IS 'Surrogate bigint FK -> tools.uses(id) ON DELETE CASCADE: the tool_use this result answers. On the wire the block carries the provider tool_use id (its `tool_use_id` field, e.g. `toolu_...`); the write path resolves that text against tools.uses.wire_tool_use_id and stores the surrogate id here. A result whose tool_use was never captured is silently dropped (INNER JOIN), so every row here has a real parent.';
COMMENT ON COLUMN tools.results.ord IS 'Zero-based position of this tool_result block within its carrying request body. Unique together with request_id.';
COMMENT ON COLUMN tools.results.content IS 'The FULL tool_result content as JSONB, verbatim from the wire (no truncation): either a bare string OR an array of typed content blocks (text, image, etc.). The command/output corpus for analysis — pair with the originating tools.uses.input via tool_use_id.';
COMMENT ON COLUMN tools.results.content_sha256 IS 'Hex SHA-256 of the full result content. Enables exact-duplicate detection and dedup of identical outputs across calls without comparing full blobs.';
COMMENT ON COLUMN tools.results.created_at IS 'Capture timestamp (when the carrying request body was processed).';

CREATE INDEX idx_tool_results_request_id ON tools.results USING btree (request_id);
CREATE INDEX idx_tool_results_errors ON tools.results USING btree (request_id) WHERE is_error;
CREATE INDEX idx_tool_results_tool_use_id ON tools.results USING btree (tool_use_id);

-- ════════════════════════════════════════════════════════════════════
-- agents — agent defs, dispatches, instances
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE agents.sources (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO agents.sources (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'observed'),
    (2, 'file_backed'),
    (3, 'dynamic');

CREATE TABLE agents.agents (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uuid uuid DEFAULT gen_random_uuid() NOT NULL,
    name text NOT NULL,
    alias text,
    definition_path text,
    frontmatter text,
    frontmatter_name text,
    frontmatter_description text,
    frontmatter_model text,
    frontmatter_skills text,
    skills text,
    description text,
    source_id bigint DEFAULT 1 NOT NULL REFERENCES agents.sources(id),
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL
);
CREATE INDEX idx_agents_source ON agents.agents USING btree (source_id);
CREATE INDEX idx_agents_name ON agents.agents USING btree (name);

CREATE TABLE agents.dispatches (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tool_use_id bigint NOT NULL REFERENCES tools.uses(id) ON DELETE CASCADE,
    agent_id bigint NOT NULL REFERENCES agents.agents(id) ON DELETE RESTRICT
);

COMMENT ON TABLE agents.dispatches IS 'One row per Agent tool_use, carrying a typed FK to agents.agents(id). ON DELETE RESTRICT on agent_id — retiring an agent must not silently invalidate dispatch history. tool_use_id -> tools.uses(id) ON DELETE CASCADE.';
COMMENT ON COLUMN agents.dispatches.id IS 'Surrogate primary key (DB-assigned identity). FK target for agents.instances.dispatch_id.';
COMMENT ON COLUMN agents.dispatches.tool_use_id IS 'Surrogate bigint FK -> tools.uses(id) ON DELETE CASCADE — the specific Agent-tool tool_use row that spawned this dispatch. Resolved from the wire id at write time (the tool_use was just inserted in the same tx); a dispatch is skipped rather than written with a dangling FK if that resolution fails.';
COMMENT ON COLUMN agents.dispatches.agent_id IS 'FK -> agents.agents(id) ON DELETE RESTRICT: the named sub-agent definition dispatched (derived from the Agent tool_use input''s subagent_type). RESTRICT preserves dispatch history — an agent definition cannot be deleted while dispatches reference it.';

CREATE INDEX idx_agent_dispatches_agent_id ON agents.dispatches USING btree (agent_id);

CREATE TABLE agents.instances (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    dispatch_id bigint NOT NULL REFERENCES agents.dispatches(id) ON DELETE CASCADE,
    agent_id bigint REFERENCES agents.agents(id) ON DELETE SET NULL,
    subagent_persona text,
    first_request_id bigint REFERENCES requests.requests(id) ON DELETE SET NULL,
    started_at timestamptz,
    created_at timestamptz DEFAULT now() NOT NULL
);

COMMENT ON TABLE agents.instances IS 'One row per spawned sub-agent instance. Links a dispatch to the sub-agent session, persona, and lifecycle start.';
COMMENT ON COLUMN agents.instances.dispatch_id IS 'FK to the agents.dispatches row that spawned this instance.';
COMMENT ON COLUMN agents.instances.agent_id IS 'FK to the named agent definition. SET NULL on agent deletion so instance history is preserved.';
COMMENT ON COLUMN agents.instances.first_request_id IS 'The id of the first request emitted by this instance. FK -> requests.requests(id) ON DELETE SET NULL. NULL until the first request arrives (set by a later UPDATE).';

CREATE INDEX idx_agent_instances_session_id ON agents.instances USING btree (session_id);
CREATE INDEX idx_agent_instances_agent_id ON agents.instances USING btree (agent_id);
CREATE INDEX agent_instances_session_persona_idx ON agents.instances USING btree (session_id, subagent_persona) WHERE (subagent_persona IS NOT NULL);

-- ════════════════════════════════════════════════════════════════════
-- overrides — prompt overrides + revisions
-- (agent_id -> agents.agents is inline: agents block precedes this one)
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE overrides.modes (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO overrides.modes (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'observe'),
    (2, 'template'),
    (3, 'replace');

CREATE TABLE overrides.sources (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO overrides.sources (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'manual'),
    (2, 'filter');

CREATE TABLE overrides.overrides (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uuid uuid DEFAULT gen_random_uuid() NOT NULL,
    prompt_sha256 text NOT NULL,
    agent_id bigint REFERENCES agents.agents(id) ON DELETE SET NULL,
    filter_rule_id bigint REFERENCES rules.rules(id) ON DELETE CASCADE,
    system_prompt_id bigint NOT NULL REFERENCES system_prompts.system_prompts(id),
    agent_name text,
    agent_definition_path text,
    label text NOT NULL,
    blocks jsonb NOT NULL,
    mode_id bigint DEFAULT 1 NOT NULL REFERENCES overrides.modes(id),
    notes text,
    version bigint DEFAULT 1 NOT NULL,
    source_id bigint DEFAULT 1 NOT NULL REFERENCES overrides.sources(id),
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL
);
CREATE INDEX idx_overrides_agent_id ON overrides.overrides USING btree (agent_id);
CREATE INDEX idx_overrides_filter_rule_id ON overrides.overrides USING btree (filter_rule_id);
CREATE INDEX idx_overrides_sysprompt_id ON overrides.overrides USING btree (system_prompt_id);
CREATE INDEX idx_overrides_agent_name ON overrides.overrides USING btree (agent_name) WHERE (agent_name IS NOT NULL);
CREATE INDEX idx_overrides_prompt_sha256 ON overrides.overrides USING btree (prompt_sha256);

CREATE TABLE overrides.revisions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uuid uuid DEFAULT gen_random_uuid() NOT NULL,
    override_id bigint NOT NULL REFERENCES overrides.overrides(id) ON DELETE CASCADE,
    version bigint NOT NULL,
    label text NOT NULL,
    blocks jsonb NOT NULL,
    mode_id bigint NOT NULL REFERENCES overrides.modes(id),
    notes text,
    created_at timestamptz DEFAULT now() NOT NULL
);
CREATE INDEX idx_override_revisions_override_id ON overrides.revisions USING btree (override_id);

-- ════════════════════════════════════════════════════════════════════
-- memory — operator turns, chunks, lessons
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE memory.turn_outcomes (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.turn_outcomes (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'unknown'),
    (2, 'accepted'),
    (3, 'corrected'),
    (4, 'abandoned'),
    (5, 'shipped');

CREATE TABLE memory.chunk_roles (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.chunk_roles (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'prompt'),
    (2, 'assistant');

CREATE TABLE memory.lesson_kinds (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.lesson_kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'correction'),
    (2, 'convention'),
    (3, 'warning'),
    (4, 'preference');

CREATE TABLE memory.lesson_sources (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.lesson_sources (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'agent'),
    (2, 'operator'),
    (3, 'copilot');

CREATE TABLE memory.lesson_stages (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.lesson_stages (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'raw'),
    (2, 'daily'),
    (3, 'long_term'),
    (4, 'archived');

CREATE TABLE memory.lesson_archived_reasons (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.lesson_archived_reasons (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'duplicate'),
    (2, 'expired'),
    (3, 'trimmed'),
    (4, 'superseded'),
    (5, 'operator');

CREATE TABLE memory.operator_turns (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    project_id bigint NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    start_request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    end_request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    outcome_evidence_request_id bigint REFERENCES requests.requests(id) ON DELETE SET NULL,
    outcome_id bigint DEFAULT 1 NOT NULL REFERENCES memory.turn_outcomes(id),
    classified_at timestamptz,
    created_at timestamptz DEFAULT now() NOT NULL,
    closed_at timestamptz,
    embedded_at timestamptz,
    -- Aggregate cache, materialized ONCE at turn-close by the Rust turn-classifier (the close
    -- site that sets closed_at), computed in one pass from the normalized tables
    -- (tools.uses, tools.results, requests.diagnostics, responses.responses, agents.dispatches,
    -- responses.stream_chunks). NULL while the turn is open. This is NOT the banned JSONB-blob
    -- recompute job and NOT a DB trigger: the turn is immutable after close, so a single
    -- compute-at-close event is drift-free. operator_turn_aggregates_v / operator_turn_text_v
    -- read these columns for closed turns and recompute live only for the one open turn.
    tool_call_count bigint,
    tool_error_count bigint,
    weasel_findings_count bigint,
    total_input_tokens bigint,
    total_output_tokens bigint,
    tools_used text[],
    agents_dispatched text[],
    files_touched text[],
    -- Full concatenated assistant text for the turn (string_agg of text stream_chunks at close).
    -- A third copy alongside responses.stream_chunks (the source of truth, never dropped) and
    -- the ANN-chunked memory.operator_turn_chunks (chunked for embedding, not the full string);
    -- justified by a single column read on every list_operator_turns recall. Full verbatim text.
    final_assistant_text text,
    -- User prompt that opened this turn (last_user_message from the start request). Written at
    -- turn-open by snapshot_request so reads (embedding, recall) need no join back to
    -- requests.requests.
    user_prompt text
);

COMMENT ON TABLE memory.operator_turns IS 'One row per Claude Code operator turn (user prompt -> next user prompt or session settle). The aggregate cache columns (tool_call_count … final_assistant_text) are materialized once at turn-close by the Rust classifier from normalized tables; operator_turn_text_v / operator_turn_aggregates_v read them for closed turns and recompute live only for the open turn.';
COMMENT ON COLUMN memory.operator_turns.start_request_id IS 'requests.requests.id of the user_prompt that opened this turn. FK ON DELETE CASCADE.';
COMMENT ON COLUMN memory.operator_turns.end_request_id IS 'requests.requests.id of the last request in this turn (tool_loop or same as start_request_id). FK ON DELETE CASCADE.';
COMMENT ON COLUMN memory.operator_turns.outcome_id IS 'Classification label FK -> memory.turn_outcomes. Default 1 (unknown).';

CREATE INDEX idx_operator_turns_project ON memory.operator_turns USING btree (project_id, created_at DESC);
CREATE INDEX idx_operator_turns_outcome_created ON memory.operator_turns USING btree (outcome_id, created_at DESC);
CREATE INDEX idx_operator_turns_start_request_id ON memory.operator_turns USING btree (start_request_id);
CREATE INDEX idx_operator_turns_end_request_id ON memory.operator_turns USING btree (end_request_id);
CREATE INDEX idx_operator_turns_evidence_request_id ON memory.operator_turns USING btree (outcome_evidence_request_id);

CREATE TABLE memory.operator_turn_chunks (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    turn_id bigint NOT NULL REFERENCES memory.operator_turns(id) ON DELETE CASCADE,
    chunk_idx integer NOT NULL,
    chunk_role_id bigint NOT NULL REFERENCES memory.chunk_roles(id),
    chunk_text text NOT NULL,
    chunk_embedding vector(384) NOT NULL,
    embedding_model text NOT NULL,
    embedded_at timestamptz DEFAULT now() NOT NULL
);
CREATE INDEX idx_otc_turn ON memory.operator_turn_chunks USING btree (turn_id);
CREATE INDEX idx_otc_embedding ON memory.operator_turn_chunks USING hnsw (chunk_embedding vector_cosine_ops) WITH (m='16', ef_construction='64');

CREATE TABLE memory.lessons (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uuid uuid DEFAULT gen_random_uuid() NOT NULL,
    project_id bigint REFERENCES projects.projects(id) ON DELETE CASCADE,
    source_turn_id bigint REFERENCES memory.operator_turns(id) ON DELETE SET NULL,
    session_id bigint REFERENCES sessions.sessions(id) ON DELETE SET NULL,
    agent_id bigint REFERENCES agents.agents(id) ON DELETE SET NULL,
    agent_instance_id bigint REFERENCES agents.instances(id) ON DELETE SET NULL,
    duplicate_of_id bigint REFERENCES memory.lessons(id) ON DELETE SET NULL,
    request_id bigint REFERENCES requests.requests(id) ON DELETE SET NULL,
    kind_id bigint NOT NULL REFERENCES memory.lesson_kinds(id),
    text text NOT NULL,
    stage_id bigint DEFAULT 1 NOT NULL REFERENCES memory.lesson_stages(id),
    source_id bigint DEFAULT 1 NOT NULL REFERENCES memory.lesson_sources(id),
    text_sha256 bytea,
    embedding vector(384),
    embedding_model text,
    embedded_at timestamptz,
    consolidation_count integer DEFAULT 0 NOT NULL,
    last_consolidated_at timestamptz,
    archived_at timestamptz,
    archived_reason_id bigint REFERENCES memory.lesson_archived_reasons(id),
    created_at timestamptz DEFAULT now() NOT NULL,
    expires_at timestamptz,
    CONSTRAINT lessons_archive_reason_consistent CHECK (((archived_reason_id IS NOT NULL) = (archived_at IS NOT NULL))),
    CONSTRAINT lessons_consolidation_count_non_negative CHECK ((consolidation_count >= 0)),
    CONSTRAINT lessons_duplicate_of_requires_archive CHECK (((duplicate_of_id IS NULL) OR (archived_reason_id = 1))),
    CONSTRAINT lessons_embedding_consistent CHECK ((((embedding IS NULL) = (embedded_at IS NULL)) AND ((embedding IS NULL) = (embedding_model IS NULL)))),
    CONSTRAINT lessons_stage_archive_consistent CHECK (((stage_id = 4) = (archived_at IS NOT NULL)))
);

COMMENT ON TABLE memory.lessons IS 'Operator/agent-authored memory entries scoped to a project. Pulled by recall_project_context. Inspection-fidelity: text is full content, never truncated.';
COMMENT ON COLUMN memory.lessons.source_turn_id IS 'Optional pointer to the operator_turn that prompted this lesson. ON DELETE SET NULL: lessons are project-level memory and must outlive any individual turn.';
COMMENT ON COLUMN memory.lessons.expires_at IS 'Optional expiry timestamp. NULL = never expires.';
COMMENT ON COLUMN memory.lessons.request_id IS 'FK -> requests.requests(id) ON DELETE SET NULL. Pointer to the originating request; NULL when no originating request applies.';

CREATE INDEX idx_lessons_project ON memory.lessons USING btree (project_id, created_at DESC);
CREATE INDEX idx_lessons_session_id ON memory.lessons USING btree (session_id);
CREATE INDEX idx_lessons_session ON memory.lessons (session_id, created_at DESC) WHERE session_id IS NOT NULL;
CREATE INDEX idx_lessons_agent_id ON memory.lessons USING btree (agent_id);
CREATE INDEX idx_lessons_duplicate_of_id ON memory.lessons USING btree (duplicate_of_id);
CREATE INDEX idx_lessons_agent_instance ON memory.lessons USING btree (agent_instance_id, created_at DESC) WHERE (agent_instance_id IS NOT NULL);
CREATE INDEX idx_lessons_daily_by_project ON memory.lessons USING btree (project_id, created_at) WHERE (stage_id = 2);
CREATE INDEX idx_lessons_raw_pending ON memory.lessons USING btree (project_id, created_at) WHERE ((stage_id = 1) AND (embedded_at IS NULL));
CREATE INDEX idx_lessons_injectable_global ON memory.lessons USING btree (agent_id, stage_id, created_at DESC) WHERE ((stage_id = ANY (ARRAY[2, 3])) AND (archived_at IS NULL) AND (project_id IS NULL));
CREATE INDEX idx_lessons_injectable_project ON memory.lessons USING btree (project_id, agent_id, stage_id, created_at DESC) WHERE ((stage_id = ANY (ARRAY[2, 3])) AND (archived_at IS NULL) AND (project_id IS NOT NULL));
CREATE INDEX idx_lessons_embedding ON memory.lessons USING hnsw (embedding vector_cosine_ops) WITH (m='16', ef_construction='64') WHERE (embedding IS NOT NULL);

-- Cross-schema FKs that cannot be inline in tables.sql.
--
-- 1. The requests↔agent_instances cycle:
--      requests -> agent_instances -> agent_dispatches -> tool_uses -> requests
--    No table-creation order resolves it; the FK is added after both tables exist.
-- 2. Forward edge: the requests schema block is created before the overrides
--    block, so requests.system_prompts.override_id can't reference
--    overrides.overrides inline.
-- 3. Forward edge: the requests block is created before the memory block, so
--    requests.requests.operator_turn_id can't reference memory.operator_turns
--    inline. The turn-classifier stamps the column after the turn row exists.
-- All three constraints are fully enforced at runtime — deferral here is purely a
-- DDL-ordering device, not a weakened FK.

ALTER TABLE ONLY requests.requests
    ADD CONSTRAINT requests_agent_instance_id_fkey
    FOREIGN KEY (agent_instance_id) REFERENCES agents.instances(id) ON DELETE SET NULL;

ALTER TABLE ONLY requests.system_prompts
    ADD CONSTRAINT request_system_prompts_override_id_fkey
    FOREIGN KEY (override_id) REFERENCES overrides.overrides(id);

-- ON DELETE SET NULL: deleting an operator turn detaches its requests (sets the
-- column NULL) rather than cascading the delete to captured request rows.
ALTER TABLE ONLY requests.requests
    ADD CONSTRAINT requests_operator_turn_id_fkey
    FOREIGN KEY (operator_turn_id) REFERENCES memory.operator_turns(id) ON DELETE SET NULL;

-- Read-API views. View names stay in the claude_proxy schema (stable Rust consumer
-- API: rehydrate.rs, persistence/fingerprint.rs, persistence/mod.rs); table references
-- are domain-namespaced (tools.*, requests.*, responses.*, memory.*, agents.*,
-- projects.*, sessions.*). "Domain schemas store, claude_proxy is the read API."

-- ────────────────────────────────────────────────────────────────────────
-- file_touches_v — thin pass-through over the capture-time columns on tools.uses.
-- No CASE, no UNION ALL, no tool-name list: the "which file / which operation"
-- decision is resolved once at capture by the Rust tool_name->path-field registry
-- (an OPEN allow-list), not guessed here. file_path/operation are full values.
-- ────────────────────────────────────────────────────────────────────────
CREATE VIEW claude_proxy.file_touches_v AS
SELECT tu.wire_tool_use_id,
       tu.request_id,
       tu.file_path,
       tu.operation
FROM tools.uses tu
WHERE tu.file_path IS NOT NULL;

-- ────────────────────────────────────────────────────────────────────────
-- operator_turn_aggregates_v — per-turn counts/sums/arrays.
-- Closed turns read the materialized cache columns on memory.operator_turns
-- (written once at close by the Rust classifier). The single open turn computes
-- live via two inline GROUP BY subqueries, each scanning requests.requests once
-- through idx_requests_operator_turn_id. tokens are separated to avoid the
-- uses×diagnostics cross-product inflating SUM(input_tokens).
-- ────────────────────────────────────────────────────────────────────────
CREATE VIEW claude_proxy.operator_turn_aggregates_v AS
SELECT ot.id AS turn_id,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.tool_call_count,       0) ELSE COALESCE(la.tool_call_count,       0) END AS tool_call_count,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.tool_error_count,      0) ELSE COALESCE(la.tool_error_count,      0) END AS tool_error_count,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.weasel_findings_count, 0) ELSE COALESCE(la.weasel_findings_count, 0) END AS weasel_findings_count,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.total_input_tokens,    0) ELSE COALESCE(lt.total_input_tokens,    0) END AS total_input_tokens,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.total_output_tokens,   0) ELSE COALESCE(lt.total_output_tokens,   0) END AS total_output_tokens,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.tools_used,         '{}') ELSE COALESCE(la.tools_used,         '{}') END AS tools_used,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.agents_dispatched,  '{}') ELSE COALESCE(la.agents_dispatched,  '{}') END AS agents_dispatched,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.files_touched,      '{}') ELSE COALESCE(la.files_touched,      '{}') END AS files_touched
FROM memory.operator_turns ot
-- la: non-token aggregates. tools.results joins by request_id (tool results live
-- in the follow-up request, not the request that emitted the tool use).
-- agents.dispatches joins through tools.uses (1:1). tools.uses, tools.results,
-- and diagnostics are each independent 1:N children of requests, creating a
-- uses×results×diagnostics cross-product per request — COUNT(DISTINCT) and
-- ARRAY_AGG(DISTINCT) give correct totals despite the duplication.
LEFT JOIN (
    SELECT r.operator_turn_id,
           COUNT(DISTINCT u.id)                                                                                     AS tool_call_count,
           COALESCE(ARRAY_AGG(DISTINCT u.tool_name ORDER BY u.tool_name) FILTER (WHERE u.tool_name IS NOT NULL), '{}'::text[]) AS tools_used,
           COUNT(DISTINCT CASE WHEN res.is_error THEN res.id END)                                                   AS tool_error_count,
           COUNT(DISTINCT CASE WHEN d.analyzer = 'weasel' THEN d.id END)                                           AS weasel_findings_count,
           COALESCE(ARRAY_AGG(DISTINCT ag.name ORDER BY ag.name) FILTER (WHERE ag.name IS NOT NULL), '{}'::text[])  AS agents_dispatched,
           COALESCE(ARRAY_AGG(DISTINCT u.file_path ORDER BY u.file_path) FILTER (WHERE u.file_path IS NOT NULL), '{}'::text[]) AS files_touched
    FROM requests.requests r
    LEFT JOIN tools.uses u ON u.request_id = r.id
    LEFT JOIN tools.results res ON res.request_id = r.id
    LEFT JOIN requests.diagnostics d ON d.request_id = r.id
    LEFT JOIN agents.dispatches ad ON ad.tool_use_id = u.id
    LEFT JOIN agents.agents ag ON ag.id = ad.agent_id
    WHERE r.operator_turn_id IS NOT NULL
    GROUP BY r.operator_turn_id
) la ON la.operator_turn_id = ot.id AND ot.closed_at IS NULL
-- lt: token sums only. responses.responses is 1:1 per request so SUM is exact.
-- Kept separate from la to avoid the uses×diagnostics fan-out inflating the sum.
LEFT JOIN (
    SELECT r.operator_turn_id,
           COALESCE(SUM(resp.input_tokens),  0) AS total_input_tokens,
           COALESCE(SUM(resp.output_tokens), 0) AS total_output_tokens
    FROM requests.requests r
    LEFT JOIN responses.responses resp ON resp.request_id = r.id
    WHERE r.operator_turn_id IS NOT NULL
    GROUP BY r.operator_turn_id
) lt ON lt.operator_turn_id = ot.id AND ot.closed_at IS NULL;

-- ────────────────────────────────────────────────────────────────────────
-- operator_turn_text_v — user prompt + concatenated assistant text per turn.
-- Closed turns read memory.operator_turns.final_assistant_text; the open turn
-- string_aggs live. Membership is r.operator_turn_id = ot.id (equality), which
-- also CLOSES the prior cross-session bug: the old range form had no session
-- guard and could string_agg another session's text into a turn.
-- ────────────────────────────────────────────────────────────────────────
CREATE VIEW claude_proxy.operator_turn_text_v AS
SELECT ot.id AS turn_id,
       -- Closed turns: read from the materialized column written at open time.
       -- Open turns: fall back to the start request (column is NULL until written).
       COALESCE(ot.user_prompt, sr.last_user_message) AS user_prompt,
       CASE WHEN ot.closed_at IS NOT NULL THEN ot.final_assistant_text
            ELSE atx.final_assistant_text END AS final_assistant_text
FROM memory.operator_turns ot
-- Only needed for the one open turn (ot.user_prompt is NULL until written at open).
LEFT JOIN requests.requests sr ON sr.id = ot.start_request_id AND ot.user_prompt IS NULL
LEFT JOIN LATERAL (
    SELECT string_agg(c.chunk_content, '' ORDER BY c.request_id, c.seq_ord) AS final_assistant_text
    FROM responses.stream_chunks c
    JOIN requests.requests r ON r.id = c.request_id
    JOIN responses.stream_delta_types dt ON dt.id = c.delta_type_id
    WHERE r.operator_turn_id = ot.id
      AND dt.code = 'text'
      AND ot.closed_at IS NULL
) atx ON true;

-- ────────────────────────────────────────────────────────────────────────
-- session_summary_v — one row per session with project-joined metadata.
-- INNER JOIN is correct: sessions.sessions.project_id is NOT NULL, so every
-- session has a project. The old LEFT JOIN + COALESCE(…, '(no project)') /
-- COALESCE(pinned, false) were dead code against that constraint.
--
-- request_count and last_models are computed via a single LEFT JOIN GROUP BY
-- (one pass over requests) instead of correlated subqueries.
-- last_path was dropped: every request hits /v1/messages so the column
-- carried no information; project_path already identifies the context.
-- ────────────────────────────────────────────────────────────────────────
CREATE VIEW claude_proxy.session_summary_v AS
SELECT s.session_uuid::text AS session_id,
       p.project_path,
       s.first_seen,
       s.last_seen,
       s.last_status,
       s.title,
       s.pinned,
       p.pinned AS project_pinned,
       COUNT(r.id)::int4 AS request_count,
       COALESCE(
           ARRAY_AGG(DISTINCT r.model ORDER BY r.model) FILTER (WHERE r.model IS NOT NULL),
           ARRAY[]::text[]
       ) AS last_models
FROM sessions.sessions s
JOIN projects.projects p ON p.id = s.project_id
LEFT JOIN requests.requests r ON r.session_id = s.id
GROUP BY s.id, p.id;

COMMENT ON VIEW claude_proxy.session_summary_v IS 'One row per session with its project-joined summary metadata. Single-pass GROUP BY on s.id, p.id — no subqueries, no LATERAL. COUNT(r.id) ignores NULL rows from the LEFT JOIN so sessions with no requests return 0. last_path dropped — always /v1/messages, no information content. Latency computed on demand against idx_requests_session_completed.';

-- ════════════════════════════════════════════════════════════════════
-- LISTEN/NOTIFY trigger functions and triggers
-- Channel: proxy.row_changed
-- Payload: JSON with table, id, op, and (for requests) session_id
-- ════════════════════════════════════════════════════════════════════

CREATE FUNCTION claude_proxy.notify_row_changed_requests()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
    FROM sessions.sessions s
    WHERE s.id = COALESCE(NEW.session_id, OLD.session_id);
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'requests',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_requests_row_changed
AFTER INSERT OR UPDATE OR DELETE ON requests.requests
FOR EACH ROW EXECUTE FUNCTION claude_proxy.notify_row_changed_requests();


CREATE FUNCTION claude_proxy.notify_row_changed_sessions()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table', 'sessions',
        'id',    COALESCE(NEW.session_uuid, OLD.session_uuid)::text,
        'op',    TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_sessions_row_changed
AFTER UPDATE ON sessions.sessions
FOR EACH ROW EXECUTE FUNCTION claude_proxy.notify_row_changed_sessions();


CREATE FUNCTION claude_proxy.notify_row_changed_filter_rules()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table', 'filter_rules',
        'id',    COALESCE(NEW.uuid, OLD.uuid)::text,
        'op',    TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_filter_rules_row_changed
AFTER INSERT OR UPDATE OR DELETE ON rules.rules
FOR EACH ROW EXECUTE FUNCTION claude_proxy.notify_row_changed_filter_rules();


CREATE FUNCTION claude_proxy.notify_row_changed_prompt_compositions()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table', 'prompt_compositions',
        'id',    COALESCE(NEW.uuid, OLD.uuid)::text,
        'op',    TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_prompt_compositions_row_changed
AFTER INSERT OR UPDATE OR DELETE ON overrides.overrides
FOR EACH ROW EXECUTE FUNCTION claude_proxy.notify_row_changed_prompt_compositions();
