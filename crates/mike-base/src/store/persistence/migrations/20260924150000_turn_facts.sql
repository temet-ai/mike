-- ════════════════════════════════════════════════════════════════════
-- JC02: memory.turn_facts — per-turn facts for judgement and steering
--
-- DRAFT. The version prefix is assigned at build time; it must sort above
-- JC01's migration and above the highest version on origin/main
-- (20260923200000_work_item_attention.sql at 4454d4ec) and anything a peer
-- has pushed since. Run `mike migrate-check`. Final file name:
-- <version>_turn_facts.sql
--
-- Why. The fleet-mirror run (night-watch-notes threads/fleet-mirror) found the
-- turn judge never sees the operator's next genuine message (47.7% of the
-- error mass on `corrected`), that sidecar text ends 2,517 of 4,089 turns
-- with text (61.6%), and that stop reason and interruption exist only in a
-- display query. This table stores those facts per closed row of
-- memory.operator_turns, built from captured data by the dossier builder
-- (crates/mike/src/dossier/build) with no LLM call.
--
-- A row is a cache of facts as of built_at. The *_at_close columns are as of
-- the turn's closed_at and are written once. JC04 rebuilds any column as of
-- another moment from the same sources.
--
-- text_cut_at and text_spans are character offsets into
-- memory.operator_turns.final_assistant_text (char_length, UTF8), never
-- copies of it (.claude/rules/inspection-fidelity.md, Layer 1).
-- checks_run carries the full Bash command of each check and ids; it is
-- derived, not sliced.
--
-- No foreign key on next_genuine_request_id / next_genuine_turn_id /
-- stop_request_id: a facts write must never fail because a referenced row
-- was pruned, the reasoning 20260829000002_llm_calls.sql gives for its ids.
--
-- Also: the feature toggle dossier_builder, default OFF. The builder reads
-- captured rows and writes derived rows in mike's own database; it mutates
-- no outbound request and sends nothing off the machine. It ships off so a
-- deploy changes nothing in live judging until the operator switches it on
-- after deploy (PATCH /api/settings/features/dossier_builder).
-- operator_set stays false so the code default wins until an operator flips
-- it (same as 20260922150001_prompt_assistant_toggle.sql).
--
-- config.feature_toggles.key has a closed CHECK (20260615000003, last
-- extended 20260922150001). BUILD-TIME CHECK: re-read origin/main's latest
-- toggle migration; if a peer added a key after 4454d4ec, add it below.
--
-- Expand-only. Idempotent.
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS memory.turn_facts (
    operator_turn_id                 bigint PRIMARY KEY REFERENCES memory.operator_turns(id) ON DELETE CASCADE,
    session_id                       bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    builder_version                  text NOT NULL,
    built_at                         timestamptz NOT NULL,
    max_request_id                   bigint NOT NULL,

    prompt_kind                      text NOT NULL,
    prompt_rule                      text,
    prompt_kind_at_close             text NOT NULL,
    prompt_rule_at_close             text,

    next_state                       text NOT NULL,
    next_genuine_request_id          bigint,
    next_genuine_turn_id             bigint,
    next_genuine_at                  timestamptz,
    next_state_at_close              text NOT NULL,
    next_genuine_request_id_at_close bigint,
    skipped_before_next              jsonb NOT NULL DEFAULT '{}'::jsonb,
    reply_window_segment_ids         bigint[] NOT NULL DEFAULT '{}',
    reclassified_at                  timestamptz,

    text_chars                       integer NOT NULL,
    text_cut_at                      integer NOT NULL,
    last_text_origin                 text,
    text_spans                       jsonb NOT NULL DEFAULT '[]'::jsonb,

    stop_request_id                  bigint,
    stop_reason                      text,
    aborted                          boolean NOT NULL,
    interrupted                      boolean NOT NULL,
    close_cause                      text NOT NULL,

    checks_run                       jsonb NOT NULL DEFAULT '[]'::jsonb,
    checks_count                     integer NOT NULL DEFAULT 0,
    checks_failed                    integer NOT NULL DEFAULT 0,

    ended_on                         text NOT NULL,
    ended_on_version                 text NOT NULL,

    session_class                    text NOT NULL,
    session_class_rule               text NOT NULL,

    CONSTRAINT turn_facts_prompt_kind_check CHECK (prompt_kind IN (
        'genuine', 'empty', 'peer-message', 'loop-wakeup', 'channel-wake',
        'task-notification', 'background-event', 'tool-loaded', 'hook-feedback',
        'compaction', 'analysis-probe', 'session-recap')),
    CONSTRAINT turn_facts_prompt_kind_at_close_check CHECK (prompt_kind_at_close IN (
        'genuine', 'empty', 'peer-message', 'loop-wakeup', 'channel-wake',
        'task-notification', 'background-event', 'tool-loaded', 'hook-feedback',
        'compaction', 'analysis-probe', 'session-recap')),
    CONSTRAINT turn_facts_prompt_rule_check
        CHECK (prompt_rule IN ('a', 'b', 'ab', 'c', 'envelope')),
    CONSTRAINT turn_facts_prompt_rule_at_close_check
        CHECK (prompt_rule_at_close IN ('a', 'b', 'ab', 'c', 'envelope')),
    CONSTRAINT turn_facts_next_state_check
        CHECK (next_state IN ('resolved', 'pending', 'none_session_settled')),
    CONSTRAINT turn_facts_next_state_at_close_check
        CHECK (next_state_at_close IN ('resolved', 'pending', 'none_session_settled')),
    CONSTRAINT turn_facts_next_resolved_has_request
        CHECK ((next_state = 'resolved') = (next_genuine_request_id IS NOT NULL)),
    CONSTRAINT turn_facts_text_cut_range
        CHECK (text_chars >= 0 AND text_cut_at >= 0 AND text_cut_at <= text_chars),
    CONSTRAINT turn_facts_stop_reason_check CHECK (stop_reason IN (
        'end_turn', 'max_tokens', 'stop_sequence', 'tool_use', 'pause_turn',
        'error', 'refusal', 'aborted')),
    CONSTRAINT turn_facts_close_cause_check
        CHECK (close_cause IN ('next_prompt', 'idle_reap', 'abort')),
    CONSTRAINT turn_facts_ended_on_check CHECK (ended_on IN (
        'report', 'question', 'handback', 'promise_or_wait', 'other', 'none')),
    CONSTRAINT turn_facts_session_class_check CHECK (session_class IN (
        'operator', 'fleet_seat', 'template', 'subagent', 'workflow', 'synthetic'))
);

COMMENT ON TABLE memory.turn_facts IS
    'Per closed memory.operator_turns row: genuine-prompt kind (J2 codes), next genuine operator message and resolution state, text_cut_at and per-origin text spans (character offsets into final_assistant_text), stop reason, interrupted/aborted, close cause, checks run, ended_on, session class. Cache as of built_at; *_at_close columns are as of closed_at and written once. Built by crates/mike/src/dossier/build, no LLM call.';
COMMENT ON COLUMN memory.turn_facts.text_cut_at IS
    'Character offset just after the last text block from a request whose origin does not end in _sidecar (J1 Q21''s definition). text_cut_at < text_chars exactly when the turn ends in sidecar text.';
COMMENT ON COLUMN memory.turn_facts.text_spans IS
    'Runs of consecutive text blocks from one origin, in string order: [{origin, start, end, first_request_id, last_request_id}], character offsets. Lets a reader remove every sidecar run, not only the trailing one.';
COMMENT ON COLUMN memory.turn_facts.reclassified_at IS
    'Last build at which prompt_kind, prompt_rule, or a non-NULL next_genuine_request_id changed. pending -> resolved is resolution, not reclassification. A verdict judged against an older next_genuine_request_id is stale (JC09).';
COMMENT ON COLUMN memory.turn_facts.ended_on IS
    'How the final primary response ends (ended-on-v1). An input for judges and steerers, never a trigger on its own: the fleet-mirror run measured that the ending phrase alone carries no signal.';

CREATE INDEX IF NOT EXISTS idx_turn_facts_session
    ON memory.turn_facts (session_id, operator_turn_id);

CREATE INDEX IF NOT EXISTS idx_turn_facts_pending
    ON memory.turn_facts (session_id)
    WHERE next_state = 'pending';

ALTER TABLE config.feature_toggles
    DROP CONSTRAINT IF EXISTS feature_toggles_key_check;

ALTER TABLE config.feature_toggles
    ADD CONSTRAINT feature_toggles_key_check
    CHECK (key IN (
        'reminders',
        'recall_injector',
        'weasel_intercept',
        'steerer',
        'lesson_worker',
        'harvester',
        'db_resync',
        'judge_worker',
        'session_judge',
        'reminder_on_token',
        'mailbox_redelivery',
        'agent_lessons',
        'file_read_compression',
        'retention_sweep',
        'proxy_tools',
        'channel_link_sweep',
        'workspace_capture_registration',
        'self_identity_reminder',
        'workspace_briefing',
        'prompt_assistant',
        'dossier_builder'
    ));

INSERT INTO config.feature_toggles (key, enabled, operator_set) VALUES
    ('dossier_builder', false, false)
ON CONFLICT (key) DO NOTHING;
