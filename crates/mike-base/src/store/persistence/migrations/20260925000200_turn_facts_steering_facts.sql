--
-- Steering facts on memory.turn_facts: two deterministic facts the turn guard (JC16) conditions
-- on, stored as of the turn's close; the third, the session class, is JC02's column. None of them
-- costs an LLM call or leaves the machine.
--
-- Why. JC00d measured that the wording of an ending carries no signal: across the 2,111 census
-- turns, endings on a promise or wait were corrected 43.3% of the time (358 turns), hand-backs 44.5%
-- (229), reports 41.4%, other endings 39.5% (plans/judgement-context/p0/RESULTS.md d1). What separates a real stall
-- from a real wait is whether anything backs the wait; what separates an unchecked "done" from a
-- checked one is whether the turn looked at its result after its last change; and whether an
-- ending is legitimate at all depends on the session being a fleet seat or template, which end
-- turns by design. mike already captures everything needed to answer all three.
--
--   wait_backing           the strongest backing found for a wait at close: a running registry
--                          work item, a scheduled wake, a live monitor, an unanswered message to a
--                          live peer, a wait the operator asked for, 'unknown' when a launch mike
--                          cannot yet read happened (Monitor, ScheduleWakeup and CronCreate until
--                          the work registry parses them), or 'none'.
--   wait_backing_refs      every backing found, whole, as evidence.
--   observed_after_change  NULL when the turn changed nothing; true when a check, run, request,
--                          browser look or render with a result followed the last change.
--   last_change_use_id, last_observation_use_id
--                          tools.uses ids, deliberately without a foreign key: the retention sweep
--                          may delete tool rows and a recorded fact must never block it.
--   (No session-class column: session_class and session_class_rule are JC02's, filled by JC02's one
--   derivation; the evidence is returned by JC02's classify and stored in each guard evaluation.)
--   steering_facts_version 0 until computed; the builder and the refill command write the current
--                          code version, so a rebuild can target stale rows.
--
-- Expand-only: ADD COLUMN IF NOT EXISTS with constant defaults, CHECKs that allow NULL, no index
-- (no reader filters on these columns yet). Idempotent.

ALTER TABLE memory.turn_facts
    ADD COLUMN IF NOT EXISTS wait_backing            text,
    ADD COLUMN IF NOT EXISTS wait_backing_refs       jsonb NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS observed_after_change   boolean,
    ADD COLUMN IF NOT EXISTS last_change_use_id      bigint,
    ADD COLUMN IF NOT EXISTS last_change_at          timestamptz,
    ADD COLUMN IF NOT EXISTS last_observation_use_id bigint,
    ADD COLUMN IF NOT EXISTS last_observation_at     timestamptz,
    ADD COLUMN IF NOT EXISTS observation_kind        text,
    ADD COLUMN IF NOT EXISTS steering_facts_version  smallint NOT NULL DEFAULT 0;

ALTER TABLE memory.turn_facts DROP CONSTRAINT IF EXISTS turn_facts_wait_backing_check;
ALTER TABLE memory.turn_facts
    ADD CONSTRAINT turn_facts_wait_backing_check
        CHECK (wait_backing IS NULL
               OR wait_backing IN ('work_item', 'scheduled_wake', 'monitor', 'peer_reply',
                                   'operator_requested', 'unknown', 'none'));

ALTER TABLE memory.turn_facts DROP CONSTRAINT IF EXISTS turn_facts_observation_kind_check;
ALTER TABLE memory.turn_facts
    ADD CONSTRAINT turn_facts_observation_kind_check
        CHECK (observation_kind IS NULL
               OR observation_kind IN ('test', 'build', 'lint', 'gate', 'run', 'http', 'browser',
                                       'render', 'deploy_poll'));

-- A change time goes with a change id, an observation time with an observation id.
ALTER TABLE memory.turn_facts DROP CONSTRAINT IF EXISTS turn_facts_change_pair_check;
ALTER TABLE memory.turn_facts
    ADD CONSTRAINT turn_facts_change_pair_check
        CHECK ((last_change_use_id IS NULL) = (last_change_at IS NULL)
               AND (last_observation_use_id IS NULL) = (last_observation_at IS NULL));

-- observed_after_change is NULL exactly when the turn made no change.
ALTER TABLE memory.turn_facts DROP CONSTRAINT IF EXISTS turn_facts_observed_needs_change_check;
ALTER TABLE memory.turn_facts
    ADD CONSTRAINT turn_facts_observed_needs_change_check
        CHECK (steering_facts_version = 0
               OR (observed_after_change IS NULL) = (last_change_use_id IS NULL));

ALTER TABLE memory.turn_facts DROP CONSTRAINT IF EXISTS turn_facts_steering_facts_version_check;
ALTER TABLE memory.turn_facts
    ADD CONSTRAINT turn_facts_steering_facts_version_check
        CHECK (steering_facts_version >= 0);

COMMENT ON COLUMN memory.turn_facts.wait_backing IS
    'F3, as of close: the strongest backing for a wait (work_item > scheduled_wake > monitor > '
    'peer_reply > operator_requested), unknown when an unregistered Monitor/ScheduleWakeup/CronCreate '
    'launch happened in the prior 24 h, none otherwise. NULL until computed.';

COMMENT ON COLUMN memory.turn_facts.wait_backing_refs IS
    'Every backing found, whole: [{source, id, kind, state, launched_at, due_at, owner_agent_id, '
    'peer_session_uuid, message_uuid, directive_id}].';

COMMENT ON COLUMN memory.turn_facts.observed_after_change IS
    'F4, as of close: NULL when the turn made no change; true when an observing call with a result '
    'followed the last change; false otherwise. Rules versioned by steering_facts_version.';

COMMENT ON COLUMN memory.turn_facts.last_change_use_id IS
    'tools.uses id of the last mutating call. No FK by design (retention may delete tool rows).';

COMMENT ON COLUMN memory.turn_facts.last_observation_use_id IS
    'tools.uses id of the last observing call after the last change. No FK by design.';

COMMENT ON COLUMN memory.turn_facts.steering_facts_version IS
    '0: steering facts not computed. Otherwise the STEERING_FACTS_VERSION that wrote them.';
