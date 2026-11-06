-- Remote Control channel-to-session link and worker-status history —
-- GitHub issues #231, #232 and the data half of #234.
--
-- `remote_control.channels.session_id` has existed since migration
-- 20260903000001 but nothing has ever written it: the mapping from a
-- `cse_` channel id to our own session was unsolved at the time. It is
-- solved now. Every `attachment` payload item kept in
-- `remote_control.call_payloads` carries `payload.sessionId`, the local
-- session UUID, alongside `payload.cwd` and `payload.version`; some
-- `system` payloads carry it too. Sampled on live traffic: 9,756 of 9,756
-- attachment payloads carried it, one distinct UUID per channel across
-- every channel that had sent one. These three columns capture that
-- identity the moment it is first seen on a channel, and `session_id` is
-- resolved from it against `sessions.sessions` the same call.
--
-- The columns are deliberately first-write-wins, not latest-write-wins: a
-- session does not change its own identity, working directory or client
-- version mid-run, and the payload item that announces them is the
-- client's own opening statement, not a running counter. Contrast
-- `worker_status`/`worker_status_at`, already latest-wins since migration
-- 20260903000001, and `post_turn_summary` below, which follows the same
-- latest-wins rule for the same reason: both are readings of what the
-- session is doing right now, not a fact about which session this is.
--
-- `post_turn_summary` is `PUT /worker`'s other shape: instead of the
-- `idle`/`running`/`requires_action` enum in `worker_status`, some calls
-- carry `external_metadata.post_turn_summary` — status_category,
-- status_detail, needs_action, recent_action — a free-text statement of
-- what just happened. Kept as the client sent it, JSONB, because the
-- shape is one client version's and nothing here needs to parse it to
-- roll it up.
--
-- `worker_status_transitions` is the history `worker_status`/
-- `worker_status_at` cannot hold on their own: the roll-up column only
-- ever shows the current reading, so "when did this session last change
-- state, and from what" had no answer. A row is appended only when the
-- incoming status differs from what was stored, set-based inside the
-- same statement that performs the roll-up (see
-- `store/persistence/remote_control/mod.rs`) — not a second round trip,
-- and not a row on every heartbeat, only on an actual transition. No
-- CHECK on `from_status`/`to_status`: `worker_status` itself carries none,
-- since the vocabulary is open and an unseen value must be captured, not
-- rejected — see migration 20260903000001's comment on that column.

ALTER TABLE remote_control.channels
    ADD COLUMN IF NOT EXISTS local_session_uuid uuid,
    ADD COLUMN IF NOT EXISTS local_cwd text,
    ADD COLUMN IF NOT EXISTS client_version text,
    ADD COLUMN IF NOT EXISTS post_turn_summary jsonb,
    ADD COLUMN IF NOT EXISTS post_turn_summary_at timestamptz;

COMMENT ON COLUMN remote_control.channels.local_session_uuid IS
    'This channel''s local session, read from the first kept attachment or '
    'system payload that carried payload.sessionId. First-write-wins: a '
    'session does not change identity mid-run, so once set this column '
    'never moves. NULL until such a payload has been seen.';

COMMENT ON COLUMN remote_control.channels.local_cwd IS
    'payload.cwd from the same payload local_session_uuid was read from. '
    'First-write-wins alongside it, for the same reason.';

COMMENT ON COLUMN remote_control.channels.client_version IS
    'payload.version (the Claude Code client version, e.g. "2.1.263") from '
    'the same payload local_session_uuid was read from. First-write-wins '
    'alongside it.';

COMMENT ON COLUMN remote_control.channels.post_turn_summary IS
    'external_metadata.post_turn_summary from the most recent PUT /worker '
    'call that carried one, kept as the client sent it. Latest-wins, like '
    'worker_status: a running statement of what the session is doing, not '
    'an identity fact. Many PUT /worker calls carry worker_status instead '
    'of this and leave it untouched.';

-- Index rather than the sessions FK's own delete-time check (session_id
-- already carries that): this is the correlation lookup migration
-- 20260903000001 anticipated — "which channel belongs to this session" —
-- now that something actually writes the column it indexes.
CREATE INDEX IF NOT EXISTS idx_rc_channels_local_session_uuid
    ON remote_control.channels (local_session_uuid)
    WHERE local_session_uuid IS NOT NULL;

CREATE TABLE IF NOT EXISTS remote_control.worker_status_transitions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    channel_id bigint NOT NULL
        REFERENCES remote_control.channels(id) ON DELETE CASCADE,

    -- NULL for a channel's first observed status (there is no prior
    -- reading to name). Open vocabulary, same as channels.worker_status —
    -- no CHECK constraint here either.
    from_status text,
    to_status text NOT NULL,

    -- The timestamp the transitioning call itself carried
    -- (worker_status_at), not wall-clock write time, so a transition
    -- observed late still dates to when the client actually changed
    -- state.
    observed_at timestamptz NOT NULL,

    -- The remote_control.calls row that carried this status, when the
    -- write path had one at hand. ON DELETE SET NULL: this table is the
    -- history the calls table cannot cheaply serve (see the header
    -- comment), so a transition must outlive the call row it came from
    -- rather than disappearing with it.
    call_id bigint REFERENCES remote_control.calls(id) ON DELETE SET NULL
);

-- The read shape this table exists for: one channel's status history,
-- newest first. Also the index the channel FK's cascade uses.
CREATE INDEX IF NOT EXISTS idx_rc_worker_status_transitions_channel_observed
    ON remote_control.worker_status_transitions (channel_id, observed_at DESC);

COMMENT ON TABLE remote_control.worker_status_transitions IS
    'One row per observed change in a channel''s worker_status — appended '
    'only when the incoming status differs from what was stored, never on '
    'every heartbeat. worker_status/worker_status_at on the channel row '
    'stay the current reading; this table is the history they cannot hold. '
    'No CHECK on from_status/to_status: the vocabulary is open, matching '
    'channels.worker_status.';
