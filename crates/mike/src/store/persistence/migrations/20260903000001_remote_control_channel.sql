-- Remote Control channel capture — GitHub issue #230, "Capture the Remote
-- Control channel".
--
-- Every Claude Code session behind the proxy opens a Remote Control channel
-- to api.anthropic.com under `/v1/code/sessions/<cse_id>/...`. Those requests
-- carry no `x-claude-code-session-id` header, so `accept()` in
-- crates/mike/src/proxy/engine.rs falls through to
-- `forward::forward_without_capture` and nothing is stored. Measured over one
-- four-hour window: 29,660 calls across 42 sessions, roughly 180,000 a day,
-- none of it captured.
--
-- The rule that produced that gap — the proxy never fabricates a session
-- identity — stays intact. These tables key on the `cse_` id parsed out of
-- the URL path, which is a real identifier the client supplies, not an
-- invented one.
--
-- Two tables.
--
--   remote_control.channels  One row per `cse_` id, carrying the rolled-up
--                            liveness facts (last heartbeat, idle seconds,
--                            worker status, delivery, stream open/close).
--                            Issue #231's liveness query and issue #234's
--                            roster read this row and never touch `calls`.
--                            Keep it that way: `calls` grows by ~180,000
--                            rows a day and a roster page must not scan it.
--
--   remote_control.calls     One row per HTTP call on the channel.
--                            Append-only in the normal path: the proxy
--                            records the row once, after the response
--                            completes. The one exception is
--                            `/worker/events/stream`, a long-lived SSE
--                            stream whose row is inserted at open
--                            (completed_at NULL) and updated at close.
--
--   remote_control.call_payloads  One row per payload item that a batch call
--                            carried and the capture policy KEPT. See the
--                            table's own comment for why kept-only.
--
-- WHICH BODIES ARE STORED IN FULL is decided in exactly one place, and it is
-- not this file: `KNOWN_ENDPOINTS` in
-- crates/mike/src/proxy/remote_control/route.rs. The schema only records the
-- outcome. `requests.requests` is already 103GB and issue #68 exists because
-- non-final request bodies took 94GB of it, so this schema declines that
-- mistake in advance — but it declines it by PAYLOAD TYPE, not by path.
--
-- A single `POST /worker/events` is a batch of up to 100 `{payload,
-- ephemeral?}` items, up to 10MB. Four of its payload types duplicate what
-- `/v1/messages` already captures at full fidelity — `stream_event`, `user`,
-- `assistant`, `message` — and those are recorded as size + sha256 only. The
-- same batches also carry `permission_request`, `permission_response`,
-- `permission_denied`, `permission_retry`, `task_status`,
-- `session_state_changed`, `session_context`, `session_memory`,
-- `todo_reminder` and `transcript_mirror`, none of which are captured
-- anywhere else in this database at all. Permission prompts and their
-- answers are invisible to us today. An UNRECOGNISED payload type is stored
-- in full: that vocabulary is one client version's and it will grow.
--
-- Because a batch mixes types, one body column per call cannot express
-- "keep these three, hash those ninety-seven". That is what
-- `remote_control.call_payloads` is for, and it is why a batch call's own
-- `request_body` is NULL while its size and digest still describe the whole
-- batch: each kept payload is stored once, in its own row, rather than twice.
--
-- `/worker/internal-events` is expected to duplicate nothing, so in practice
-- its body is stored in full. That is an expectation about its content, not
-- an exemption: the payload-type rule is applied uniformly, so a body whose
-- single payload turns out to be one of the four duplicate types would be
-- elided like any other. A non-empty payload array on this endpoint is
-- therefore correct behaviour rather than a bug.
--
-- Its `is_compaction`, `preserved_event_ids` and `session_agent_id` fields are
-- promoted to typed columns below, because they are the reason the endpoint
-- is worth capturing and they answer no query while sitting inside a bytea.
-- They are parsed from the envelope beside the payload rather than from the
-- payload itself, so they survive even in the case above where the body is
-- elided.
--
-- Reading a row: `request_body IS NULL AND request_body_size IS NOT NULL`
-- means a body existed and the policy chose not to store its bytes.
-- `request_body_size IS NULL` means there was no body at all (a GET). The
-- same reading applies to the response columns. No column here ever holds a
-- truncated or summarized copy of anything — the CHECK constraints below
-- enforce that a stored body's length equals its recorded size, so a slice
-- cannot be written even by accident.

CREATE SCHEMA IF NOT EXISTS remote_control;

-- ════════════════════════════════════════════════════════════════════
-- channels — one row per cse_ id, with the liveness roll-up
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS remote_control.channels (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

    -- The `cse_` id from `/v1/code/sessions/<cse_id>/...`. The channel's
    -- natural key, present on every call. Not a content hash — it is an
    -- identifier the client mints and repeats, so UNIQUE here is correct and
    -- carries none of the write-path cost of indexing a digest.
    cse_id text NOT NULL UNIQUE,

    -- Our own session, once something can correlate the two. Deliberately
    -- nullable, deliberately unbackfilled, and written by nothing today:
    -- mapping a `cse_` id to a session UUID is not solved. The client's own
    -- debug log carries the pairing, but only when debug logging is on. A
    -- channel row is worth keeping without a session, so ON DELETE SET NULL
    -- rather than CASCADE — a swept session must not take the channel's
    -- liveness history with it.
    session_id bigint REFERENCES sessions.sessions(id) ON DELETE SET NULL,

    -- The `session_id` field inside the payloads, which is NOT a route to
    -- the column above. The client sets it from the last segment of the
    -- channel URL (`this.sessionId = pathname.split('/').pop()` in the
    -- 2.1.259 client), so it repeats the `cse_` id already in the path. It
    -- is stored because it is the client's own statement of its identity: if
    -- it ever stops matching cse_id, that divergence is the correlation
    -- route we have been missing, and a stored column makes it visible
    -- instead of leaving it to be rediscovered.
    client_session_id text,

    -- The worker generation the client believes it is addressing; the client
    -- parses it with parseInt and answers a mismatch with `stale_worker_epoch`.
    -- Returned by POST /bridge and echoed on most worker calls.
    worker_epoch bigint,

    first_seen_at timestamptz NOT NULL DEFAULT now(),
    -- Touched by every call on the channel, so this row takes ~180,000
    -- updates a day across all channels. See the storage parameters below.
    last_seen_at timestamptz NOT NULL DEFAULT now(),

    -- POST /worker/heartbeat, roughly every 20 seconds per session.
    last_heartbeat_at timestamptz,
    -- `idle_seconds` from the heartbeat body, sampled by the client's own
    -- idleTracker. This is a measure of session idleness that does not
    -- depend on the agent inside the session cooperating, which is why it is
    -- a typed column and not a field inside a stored body: issues #225 and
    -- #231 both filter and sort on it.
    last_idle_seconds integer,
    -- `current_interval_seconds` from the same heartbeat — the cadence the
    -- client says it is using. Without it, "this session is overdue" has to
    -- assume twenty seconds; with it, overdue is measured against the
    -- interval the client actually announced.
    last_heartbeat_interval_seconds integer,

    -- PUT /worker. Observed values are `idle` and `requires_action`, and the
    -- client binary shows only those two literals — but the set is treated
    -- as OPEN. No CHECK constraint and no lookup-table FK: either one would
    -- reject an unseen value and drop the call that carried it, which is the
    -- opposite of what a capture table should do. If the set later proves
    -- closed and behaviour attaches to the values, promote it then.
    worker_status text,
    worker_status_at timestamptz,

    -- POST /worker/events/delivery — the phone acknowledging an inbound event.
    last_delivery_at timestamptz,
    -- GET /worker/events/stream — the long-lived inbound stream carrying
    -- phone replies. Both timestamps together are the reply-channel-health
    -- field issue #234 needs.
    stream_opened_at timestamptz,
    stream_closed_at timestamptz,

    CONSTRAINT channels_last_idle_seconds_nonneg
        CHECK (last_idle_seconds IS NULL OR last_idle_seconds >= 0),
    CONSTRAINT channels_heartbeat_interval_positive
        CHECK (last_heartbeat_interval_seconds IS NULL
               OR last_heartbeat_interval_seconds > 0)
)
-- fillfactor leaves free space on each page so the every-call `last_seen_at`
-- update can be a heap-only tuple update instead of a new index entry. The
-- autovacuum settings override defaults that are tuned for large tables: at
-- the default scale factor a few-hundred-row table taking 180,000 updates a
-- day would sit on dead tuples far longer than it should.
WITH (fillfactor = 70,
      autovacuum_vacuum_scale_factor = 0.05,
      autovacuum_vacuum_threshold = 25);

-- Correlation lookups (`which channel belongs to this session`) and the
-- sessions FK's own delete-time check. Partial because the column is NULL
-- for every channel until something solves the cse->session mapping.
CREATE INDEX IF NOT EXISTS idx_rc_channels_session_id
    ON remote_control.channels (session_id)
    WHERE session_id IS NOT NULL;

-- Time-range scans over channel creation. BRIN, and on `first_seen_at`
-- rather than `last_seen_at`, for two reasons: first_seen_at is append-
-- ordered so the block ranges stay tight, and it is never updated, so
-- indexing it does not cost the heap-only-tuple updates that the roll-up
-- columns depend on.
CREATE INDEX IF NOT EXISTS brin_rc_channels_first_seen_at
    ON remote_control.channels USING brin (first_seen_at) WITH (pages_per_range = '32');

-- No index on last_seen_at, last_heartbeat_at, worker_status or
-- last_idle_seconds, and that is deliberate rather than an omission. Every
-- one of them changes on the hot path, and any index over a changing column
-- disables the heap-only-tuple optimisation for that update — 180,000 index
-- insertions a day, forever, to speed up an ORDER BY over a table that holds
-- one row per session. The roster and liveness queries sort a few hundred
-- rows in memory instead.

COMMENT ON TABLE remote_control.channels IS
    'One row per Remote Control `cse_` channel, with the liveness facts '
    'rolled up so the issue #231 liveness query and the issue #234 roster '
    'never scan remote_control.calls. session_id is nullable by design — '
    'the cse->session mapping is unsolved and deliberately unbackfilled.';

COMMENT ON COLUMN remote_control.channels.last_idle_seconds IS
    'idle_seconds from the most recent POST /worker/heartbeat body, sampled '
    'by the client idleTracker. Agent-independent measure of how long the '
    'session has been idle.';

COMMENT ON COLUMN remote_control.channels.client_session_id IS
    'The session_id field carried inside the payloads. The 2.1.259 client '
    'derives it from the last segment of the channel URL, so it repeats '
    'cse_id and is NOT a mapping to sessions.sessions. Stored so a future '
    'divergence is visible.';

COMMENT ON COLUMN remote_control.channels.worker_status IS
    'Most recent worker_status from PUT /worker. Open set, no CHECK: '
    'observed values are idle and requires_action, but an unseen value must '
    'be captured rather than rejected.';

-- ════════════════════════════════════════════════════════════════════
-- calls — one row per HTTP call on a channel
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS remote_control.calls (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    channel_id bigint NOT NULL
        REFERENCES remote_control.channels(id) ON DELETE CASCADE,

    -- Reuses the existing requests.methods vocabulary rather than adding a
    -- parallel one; the codes are the nine HTTP methods and this table wants
    -- the same four of them requests.requests already uses.
    method_id bigint NOT NULL REFERENCES requests.methods(id),

    -- The suffix after `/v1/code/sessions/<cse_id>` — `/worker/heartbeat`,
    -- `/worker/events`, `/bridge`. Stored as text rather than as an id into
    -- a vocabulary table, and the volume argues the other way (~65M rows a
    -- year at ~20 bytes of repeated text). A vocabulary table loses on the
    -- write path: resolving an unseen path needs an upsert against a handful
    -- of shared rows, and every session's every call would take a row lock
    -- on the same `/worker/heartbeat` row. That is the exact contention
    -- shape that produced the concurrent-write incident in the projects
    -- upsert CTE. Repeated text is the cheaper mistake.
    path text NOT NULL,

    http_status integer,

    -- Full request bytes, or NULL when BODY_CAPTURE_POLICY elides them (see
    -- the header). Never a slice: the CHECK below refuses any value whose
    -- length disagrees with request_body_size.
    request_body bytea,
    -- Raw 32-byte digest, not 65 characters of hex. At this table's volume
    -- the hex form costs about 2GB a year for a value nothing joins on,
    -- matches on, or displays raw. This is a fingerprint, never a key: it is
    -- not indexed, is not an ON CONFLICT target, and nothing resolves an id
    -- from it.
    request_body_sha256 bytea,
    -- NULL means there was no request body at all. 0 means there was an
    -- empty one (POST /bridge sends `{}`; a GET sends nothing).
    request_body_size integer,

    response_body bytea,
    response_body_sha256 bytea,
    response_body_size integer,

    created_at timestamptz NOT NULL DEFAULT now(),
    -- NULL while a call is still open. Only `/worker/events/stream` is ever
    -- observed in that state; every other call is recorded once, complete.
    completed_at timestamptz,

    -- Batch accounting, NULL for a call that carries no payload batch.
    -- payload_count is every item the batch held; payload_kept_count is how
    -- many became call_payloads rows. The difference is the duplicated
    -- transcript content we deliberately did not store a second time.
    payload_count integer,
    payload_kept_count integer,
    -- Item counts by payload type, INCLUDING the types whose bytes were
    -- dropped — the only surviving record that a `stream_event` was ever
    -- here. JSONB rather than a typed column or a bridge table because the
    -- type vocabulary is open and sparse and this is descriptive only:
    -- nothing joins, sorts or filters on it, and filtering by type is done
    -- against call_payloads, which holds a row per kept item. If a
    -- containment query on the dropped types is ever wanted, a GIN index
    -- serves `payload_type_counts ? 'stream_event'` without reshaping this.
    payload_type_counts jsonb,

    -- From `POST /worker/internal-events`, NULL on every other path. These
    -- three are promoted out of the stored body because they are what makes
    -- that endpoint worth capturing and a bytea answers no query. Nullable
    -- columns on a table where ~78% of rows never set them cost a null-bitmap
    -- bit each, not a column width.
    --
    -- is_compaction with preserved_event_ids says exactly which events
    -- survived a context compaction, which nothing else in this database can
    -- answer; session_agent_id names the subagent that produced the event.
    is_compaction boolean,
    session_agent_id text,
    -- text[] rather than uuid[] deliberately: this is a capture table and an
    -- id that is not a well-formed uuid must be recorded, not rejected by a
    -- cast. `= ANY(preserved_event_ids)` answers "did event X survive"; a GIN
    -- index is available if that becomes a hot query.
    preserved_event_ids text[],

    CONSTRAINT calls_request_sha_is_sha256
        CHECK (request_body_sha256 IS NULL OR octet_length(request_body_sha256) = 32),
    CONSTRAINT calls_response_sha_is_sha256
        CHECK (response_body_sha256 IS NULL OR octet_length(response_body_sha256) = 32),
    CONSTRAINT calls_request_body_size_nonneg
        CHECK (request_body_size IS NULL OR request_body_size >= 0),
    CONSTRAINT calls_response_body_size_nonneg
        CHECK (response_body_size IS NULL OR response_body_size >= 0),
    -- The fidelity guard. A stored body must be the whole body: if the bytes
    -- are present their length has to equal the size recorded from the wire.
    -- A truncated or previewed copy cannot be written here even by mistake.
    CONSTRAINT calls_request_body_is_whole
        CHECK (request_body IS NULL OR octet_length(request_body) = request_body_size),
    CONSTRAINT calls_response_body_is_whole
        CHECK (response_body IS NULL OR octet_length(response_body) = response_body_size),
    CONSTRAINT calls_payload_counts_nonneg
        CHECK ((payload_count IS NULL OR payload_count >= 0)
               AND (payload_kept_count IS NULL OR payload_kept_count >= 0)),
    -- Kept can never exceed carried, and the two are set together or not at all.
    CONSTRAINT calls_payload_kept_within_count
        CHECK ((payload_count IS NULL) = (payload_kept_count IS NULL)
               AND (payload_count IS NULL OR payload_kept_count <= payload_count))
);

-- The per-channel call history behind the inspector's channel view, and the
-- index the channel FK's cascade uses.
CREATE INDEX IF NOT EXISTS idx_rc_calls_channel_created_at
    ON remote_control.calls (channel_id, created_at DESC);

-- Append-only time series; BRIN matches requests.requests, llm.calls and
-- steering.interventions, and is what a future retention sweep will scan.
CREATE INDEX IF NOT EXISTS brin_rc_calls_created_at
    ON remote_control.calls USING brin (created_at) WITH (pages_per_range = '32');

-- method_id carries no index, matching requests.requests.method_id. The
-- column holds nine distinct values over tens of millions of rows, so a
-- btree over it would never be chosen; the usual reason to index a foreign
-- key — making the parent's delete check cheap — does not apply to a seeded
-- vocabulary table whose rows are never deleted.

COMMENT ON TABLE remote_control.calls IS
    'One row per HTTP call on a Remote Control channel. Append-only except '
    'for /worker/events/stream, whose row opens with completed_at NULL and '
    'is closed later. Which paths store full bodies is decided by '
    'BODY_CAPTURE_POLICY in proxy/remote_control/route.rs, not here.';

COMMENT ON COLUMN remote_control.calls.request_body IS
    'Full request bytes, or NULL. NULL with a non-NULL request_body_size '
    'means a body existed and its bytes were not stored whole here; NULL with '
    'a NULL size means the call carried no body. For a batch call the bytes '
    'live in remote_control.call_payloads, one row per kept item, and the '
    'size and digest here still describe the whole batch — so the set of '
    'payloads can be proven complete, but the batch envelope itself cannot be '
    're-emitted byte for byte. That is the one fidelity limit in this schema '
    'and it buys not storing every kept payload twice.';

-- ════════════════════════════════════════════════════════════════════
-- call_payloads — one row per KEPT item of a batch call
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS remote_control.call_payloads (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    call_id bigint NOT NULL
        REFERENCES remote_control.calls(id) ON DELETE CASCADE,

    -- Position within the batch, zero-based, preserved even though dropped
    -- items get no row: a stored permission_request still reads as item 47 of
    -- a 100-item batch, so what it sat among is not lost.
    ordinal integer NOT NULL,

    -- The client's own payload `type`. Open set, no CHECK and no lookup-table
    -- FK: the vocabulary is one client version's and an unseen type must be
    -- captured rather than rejected. Indexed, because this is the column
    -- issues #232 and #233 filter on — the whole reason this table exists
    -- rather than a rewritten batch body.
    payload_type text NOT NULL,

    -- The item's exact original bytes, sliced out of the request body with no
    -- reserialization. NOT NULL because a row exists only for an item whose
    -- bytes we kept — the "stored but empty" state is unrepresentable here by
    -- construction rather than by convention.
    body bytea NOT NULL,
    body_sha256 bytea NOT NULL,
    body_size integer NOT NULL,

    -- Copied from the parent call rather than joined to it. This is a
    -- deliberate denormalization: the type-scoped, time-ranged queries #232
    -- and #233 need would otherwise join a table heading for tens of millions
    -- of rows to answer a question about a much smaller one.
    created_at timestamptz NOT NULL,

    CONSTRAINT call_payloads_ordinal_nonneg CHECK (ordinal >= 0),
    CONSTRAINT call_payloads_sha_is_sha256 CHECK (octet_length(body_sha256) = 32),
    CONSTRAINT call_payloads_body_size_nonneg CHECK (body_size >= 0),
    -- The same fidelity guard the parent carries: a stored payload is the
    -- whole payload, so a slice cannot be written even by accident.
    CONSTRAINT call_payloads_body_is_whole CHECK (octet_length(body) = body_size),
    -- One row per item of a call. Also the index the call_id foreign key uses
    -- and the one an ordered per-call read walks, so it earns its keep three
    -- times over.
    CONSTRAINT call_payloads_one_row_per_item UNIQUE (call_id, ordinal)
);

CREATE INDEX IF NOT EXISTS idx_rc_call_payloads_type_created_at
    ON remote_control.call_payloads (payload_type, created_at DESC);

COMMENT ON TABLE remote_control.call_payloads IS
    'One row per payload item of a batch Remote Control call that the capture '
    'policy kept. Rows are NOT written for items whose bytes were dropped as '
    'duplicates of /v1/messages: /worker/events runs ~76,000 calls a day at up '
    'to 100 items each, so a row per item would be billions a year, almost all '
    'of them bodyless stream_event frames answering no question. The parent '
    'call carries payload_count, payload_kept_count and payload_type_counts, '
    'so what was dropped is still counted by type.';

COMMENT ON COLUMN remote_control.call_payloads.body IS
    'The item''s exact original bytes, sliced from the request body with no '
    'reserialization. NOT NULL: a row exists only for a kept item.';

COMMENT ON COLUMN remote_control.calls.request_body_sha256 IS
    'Raw 32-byte SHA-256 of the request body. A fingerprint for comparing '
    'two calls, never a key: not indexed, never an ON CONFLICT target, and '
    'no id is ever resolved from it.';
