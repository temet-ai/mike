-- ════════════════════════════════════════════════════════════════════
-- memory.ask_ledger_entries — per-session accreted ask (issue #113)
-- ════════════════════════════════════════════════════════════════════
--
-- One row per operator statement of intent within a session's work-thread:
-- the initial ask, then every subsequent clarification or correction. Read
-- oldest-first, the rows reconstruct what the operator actually asked for
-- as it accreted turn over turn — the judge/steering consumer (unit 113b)
-- reads this instead of re-deriving intent from raw prompt text each time.
--
-- operator_turn_id is nullable: the append call-site (113b) writes an entry
-- as soon as the triggering user prompt is captured, before the turn-close
-- classifier materializes the memory.operator_turns row.

CREATE TABLE memory.ask_ledger_entries (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    operator_turn_id bigint REFERENCES memory.operator_turns(id) ON DELETE SET NULL,
    kind text NOT NULL,
    content text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ask_ledger_entries_kind_check CHECK (kind IN ('initial', 'clarification', 'correction'))
);

COMMENT ON TABLE memory.ask_ledger_entries IS 'Per-session ask ledger: initial ask + subsequent clarifications/corrections, oldest-first, one row per operator statement of intent.';
COMMENT ON COLUMN memory.ask_ledger_entries.operator_turn_id IS 'NULL until the triggering operator turn closes and the turn-classifier materializes memory.operator_turns; backfilled by the append call-site (113b), never by a trigger.';
COMMENT ON COLUMN memory.ask_ledger_entries.content IS 'Stripped, machine-text-excluded user prompt — full fidelity, no truncation (.claude/rules/inspection-fidelity.md).';

CREATE INDEX idx_ask_ledger_entries_session_created ON memory.ask_ledger_entries USING btree (session_id, created_at);
CREATE INDEX idx_ask_ledger_entries_operator_turn_id ON memory.ask_ledger_entries USING btree (operator_turn_id);
