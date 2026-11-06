-- Add human-readable description column to the three steering vocabulary
-- lookup tables so the UI can display term meanings without hardcoding them.
--
-- Pure DDL + seed UPDATEs; no query! macros reference these tables' columns
-- (verdict/authority/mode codes are written by string literals in steering.rs),
-- so no .sqlx cache refresh is needed.

ALTER TABLE steering.authorities ADD COLUMN IF NOT EXISTS description text NOT NULL DEFAULT '';
ALTER TABLE steering.modes       ADD COLUMN IF NOT EXISTS description text NOT NULL DEFAULT '';
ALTER TABLE steering.verdicts    ADD COLUMN IF NOT EXISTS description text NOT NULL DEFAULT '';

-- Authorities (confirmed from crates/claude-proxy/src/steerer/refocus.rs
-- build_refocus_message authority prompt section)
UPDATE steering.authorities SET description = 'Supply missing context or facts to the session.'       WHERE code = 'supply_context';
UPDATE steering.authorities SET description = 'Call out the drift and point attention back to the thread.' WHERE code = 'redirect';
UPDATE steering.authorities SET description = 'Issue a direct instruction to the session.'             WHERE code = 'command';

-- Modes (confirmed from refocus.rs steer-author prompt mode section)
UPDATE steering.modes SET description = 'Adds a nudge alongside the conversation.' WHERE code = 'augment';
UPDATE steering.modes SET description = 'Takes a stronger, driving steering role.'  WHERE code = 'drive';

-- Verdicts (confirmed from crates/claude-proxy/src/steerer/fleet.rs):
--
-- fired               — send_steerer_message succeeds → SteeringFired event emitted (fleet.rs:804-818)
-- suppressed_debounce — DebounceLedger::check_and_record_fire returns false when remaining > 0
--                       (fleet.rs DebounceLedger impl, debounce_turns cooldown)
-- suppressed_arb.     — fleet loop breaks on first fire; lower-priority steerers are not evaluated
--                       (fleet.rs evaluate_and_maybe_fire, priority ASC/id ASC ordering)
-- no_drift            — LLM verdict returns drifting=false or confidence < threshold (fleet.rs:~560-570)
-- gate_miss           — gate() returns None: no AvoidingWork findings and off-anchor heuristic did not
--                       fire (fleet.rs:402-408, gate.rs gate())
-- failed_closed       — LLM call errors; fleet logs "skipping steerer (fail-closed)" and continues
--                       (fleet.rs:584-594)
UPDATE steering.verdicts SET description = 'The steerer intervened — injected a [bigbro steerer] nudge into the session this turn.'                                          WHERE code = 'fired';
UPDATE steering.verdicts SET description = 'Would have fired, but held back because it already fired within the last debounce_turns turns (re-fire cooldown).'                WHERE code = 'suppressed_debounce';
UPDATE steering.verdicts SET description = 'Would have fired, but a higher-priority steerer fired this turn — at most one steerer fires per turn (priority ASC, id ASC).'    WHERE code = 'suppressed_arbitration';
UPDATE steering.verdicts SET description = 'The LLM judge found no drift from the thread anchor — no intervention needed.'                                                   WHERE code = 'no_drift';
UPDATE steering.verdicts SET description = 'The cheap pre-LLM gate did not trigger, so no drift evaluation ran this turn.'                                                   WHERE code = 'gate_miss';
UPDATE steering.verdicts SET description = 'The steerer''s evaluation could not complete (LLM error/timeout) and deliberately did nothing — fail-safe.'                     WHERE code = 'failed_closed';
