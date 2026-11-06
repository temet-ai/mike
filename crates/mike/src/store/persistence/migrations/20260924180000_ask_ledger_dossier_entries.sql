-- ════════════════════════════════════════════════════════════════════
-- JC03: ask-ledger widening, and memory.dossier_entries
--
-- 1. memory.ask_ledger_entries
--    - No row was ever linked to its turn: the only writer passes
--      operator_turn_id = NULL (steerer/fleet/ask_ledger.rs), and on
--      2026-09-24 0 of 7,033 rows were linked. The writer runs at capture,
--      before the turn row exists, so the link is made later by the dossier
--      builder through request_id, which the writer does know.
--    - request_id: the request that carried the ask. No FK: a ledger write
--      must never fail on a missing or pruned request row (the reasoning of
--      20260829000002_llm_calls.sql). Unique when present, so the steerer
--      path and the builder path cannot both insert the same ask.
--    - redirect: a kind only the judge (JC09/JC12) writes; allowed now.
--    - kind_source: who set kind (keyword classifier, judge, operator).
--    - keyword_kind / kind_updated_at: the keyword kind is kept when a judge
--      or operator later changes kind, so the ledger can be rebuilt as of any
--      moment (as of T: kind if kind_updated_at <= T, else keyword_kind, else
--      kind for rows written before this migration).
--
-- 2. memory.dossier_entries: facts about a session that have no other home.
--    kind = directive (what the operator authorised or forbade, data.class
--    and data.polarity) or promise (a turn that ended on a promise or wait,
--    with its backing read from agents.work_items). Every row names its
--    provenance in source_ref (the rule memory_harvester follows). Rows are
--    never deleted and change at most once, from open to a terminal status;
--    effective_at/recorded_at and closed_at/close_recorded_at let a reader
--    rebuild what was known at any past moment. No sha column; dedup_key is
--    plain text built from ids (.claude/rules/sql-set-based.md).
--
-- New CHECKs on the existing table are added NOT VALID and then validated,
-- so the ALTER holds its lock only for the catalogue change. A 5 s
-- lock_timeout bounds the wait.
--
-- Expand-only. Idempotent.
-- ════════════════════════════════════════════════════════════════════

SET LOCAL lock_timeout = '5s';

ALTER TABLE memory.ask_ledger_entries
    DROP CONSTRAINT IF EXISTS ask_ledger_entries_kind_check;

ALTER TABLE memory.ask_ledger_entries
    ADD CONSTRAINT ask_ledger_entries_kind_check
    CHECK (kind IN ('initial', 'clarification', 'correction', 'redirect')) NOT VALID;

ALTER TABLE memory.ask_ledger_entries
    VALIDATE CONSTRAINT ask_ledger_entries_kind_check;

ALTER TABLE memory.ask_ledger_entries
    ADD COLUMN IF NOT EXISTS request_id bigint,
    ADD COLUMN IF NOT EXISTS kind_source text NOT NULL DEFAULT 'keyword',
    ADD COLUMN IF NOT EXISTS keyword_kind text,
    ADD COLUMN IF NOT EXISTS kind_updated_at timestamptz;

ALTER TABLE memory.ask_ledger_entries
    DROP CONSTRAINT IF EXISTS ask_ledger_entries_kind_source_check;

ALTER TABLE memory.ask_ledger_entries
    ADD CONSTRAINT ask_ledger_entries_kind_source_check
    CHECK (kind_source IN ('keyword', 'judge', 'operator')) NOT VALID;

ALTER TABLE memory.ask_ledger_entries
    VALIDATE CONSTRAINT ask_ledger_entries_kind_source_check;

ALTER TABLE memory.ask_ledger_entries
    DROP CONSTRAINT IF EXISTS ask_ledger_entries_keyword_kind_check;

ALTER TABLE memory.ask_ledger_entries
    ADD CONSTRAINT ask_ledger_entries_keyword_kind_check
    CHECK (keyword_kind IN ('initial', 'clarification', 'correction')) NOT VALID;

ALTER TABLE memory.ask_ledger_entries
    VALIDATE CONSTRAINT ask_ledger_entries_keyword_kind_check;

CREATE UNIQUE INDEX IF NOT EXISTS uq_ask_ledger_entries_request
    ON memory.ask_ledger_entries (request_id)
    WHERE request_id IS NOT NULL;

COMMENT ON COLUMN memory.ask_ledger_entries.operator_turn_id IS
    'The operator turn opened by request_id. Linked by the dossier builder (crates/mike/src/dossier/entries) after the turn row exists; the append site runs at capture and cannot know it. NULL for rows whose request never opened a turn or could not be matched.';
COMMENT ON COLUMN memory.ask_ledger_entries.request_id IS
    'requests.requests.id of the prompt that carried the ask. No FK by design. Unique when present.';
COMMENT ON COLUMN memory.ask_ledger_entries.kind_source IS
    'Who set kind: keyword (classify_ask_kind), judge (turn verdict upgrade, JC09/JC12), operator (a gold correction).';
COMMENT ON COLUMN memory.ask_ledger_entries.keyword_kind IS
    'The keyword classifier''s kind at insert, never changed. NULL on rows written before JC03 (their kind is the keyword kind).';

CREATE TABLE IF NOT EXISTS memory.dossier_entries (
    id                 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id         bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    operator_turn_id   bigint REFERENCES memory.operator_turns(id) ON DELETE SET NULL,
    kind               text NOT NULL,
    data               jsonb NOT NULL DEFAULT '{}'::jsonb,
    status             text NOT NULL DEFAULT 'open',
    source             text NOT NULL,
    source_ref         text NOT NULL,
    dedup_key          text NOT NULL,
    extractor_version  text NOT NULL,
    effective_at       timestamptz NOT NULL,
    recorded_at        timestamptz NOT NULL DEFAULT now(),
    closed_at          timestamptz,
    close_recorded_at  timestamptz,
    closed_by          text,
    CONSTRAINT dossier_entries_kind_check CHECK (kind IN ('directive', 'promise')),
    CONSTRAINT dossier_entries_status_check
        CHECK (status IN ('open', 'kept', 'broken', 'withdrawn', 'superseded')),
    CONSTRAINT dossier_entries_source_check CHECK (source IN ('capture', 'judge', 'operator')),
    CONSTRAINT dossier_entries_source_ref_check CHECK (source_ref ~
        '^(requests\.requests|memory\.operator_turns|agents\.work_items|agents\.work_item_attention|memory\.ask_ledger_entries):[0-9]+$'),
    CONSTRAINT dossier_entries_closed_by_check CHECK (closed_by IS NULL OR closed_by ~
        '^(requests\.requests|memory\.operator_turns|agents\.work_items|agents\.work_item_attention|memory\.ask_ledger_entries|memory\.dossier_entries):[0-9]+$'),
    CONSTRAINT dossier_entries_close_consistent CHECK (
        (status = 'open') = (closed_at IS NULL)
        AND (closed_at IS NULL) = (close_recorded_at IS NULL)
        AND (closed_at IS NULL) = (closed_by IS NULL)),
    CONSTRAINT uq_dossier_entries_kind_dedup UNIQUE (kind, dedup_key)
);

COMMENT ON TABLE memory.dossier_entries IS
    'Directives (data.class, data.polarity, data.span into the source prompt) and promises (data.span into the turn text, data.backing from agents.work_items, data.registry_coverage; no backed or unbacked verdict: wait backing is memory.turn_facts.wait_backing, JC15 F3) per session. Promises are phrase-selected (ended_on = promise_or_wait) and are not a complete record of waits. Written by the dossier builder (source=capture), later by the judge (source=judge) or operator (source=operator). Never deleted; status changes at most once, from open. As of T: visible when recorded_at <= T, open unless close_recorded_at <= T.';
COMMENT ON COLUMN memory.dossier_entries.source_ref IS
    'Provenance as <schema.table>:<id>, required. Directive: the prompt request. Promise: the stop request of the turn that made it.';
COMMENT ON COLUMN memory.dossier_entries.dedup_key IS
    'request:<id>:<class>:<polarity> for a directive, turn:<operator_turn_id> for a promise. Plain text from ids, not a hash.';

CREATE INDEX IF NOT EXISTS idx_dossier_entries_session_effective
    ON memory.dossier_entries (session_id, effective_at);

CREATE INDEX IF NOT EXISTS idx_dossier_entries_open
    ON memory.dossier_entries (session_id)
    WHERE status = 'open';
