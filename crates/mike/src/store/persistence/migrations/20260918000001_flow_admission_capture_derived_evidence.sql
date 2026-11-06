-- Widens the admitted branch of sessions_flow_admission_evidence_check
-- (20260910000001_session_flow_capture.sql) beyond the single hook-asserted
-- combination. Two new evidence combinations, both under flow_producer =
-- 'mike': flow_start_source = 'capture' is stamped inline by mike's own
-- capture path (id_resolvers.resolve_session_id) the moment it creates a
-- session_uuid's first-ever sessions.sessions row and that row's first
-- captured request carries exactly one message; flow_start_source =
-- 'backfill' is stamped by the backfill-flow-admission subcommand for a
-- pre-existing null-columns row where the earliest captured request can
-- still be shown, after the fact, to be message 1. The original hook-only
-- combination (flow_producer = 'claude_code') is unchanged.
ALTER TABLE sessions.sessions
    DROP CONSTRAINT IF EXISTS sessions_flow_admission_evidence_check;

ALTER TABLE sessions.sessions ADD CONSTRAINT sessions_flow_admission_evidence_check
CHECK (
    (flow_capture_version IS NULL AND flow_admitted_at IS NULL
     AND flow_producer IS NULL AND flow_hook_event_name IS NULL
     AND flow_start_source IS NULL)
    OR
    (flow_capture_version IS NOT NULL AND flow_capture_version BETWEEN 1 AND 65535
     AND flow_admitted_at IS NOT NULL
     AND flow_producer IS NOT NULL
     AND flow_hook_event_name IS NOT NULL
     AND flow_start_source IS NOT NULL
     AND (
         (flow_producer = 'claude_code' AND flow_hook_event_name = 'SessionStart'
          AND flow_start_source = 'startup')
         OR
         (flow_producer = 'mike' AND flow_hook_event_name = 'capture'
          AND flow_start_source IN ('capture', 'backfill'))
     ))
) NOT VALID;

ALTER TABLE sessions.sessions VALIDATE CONSTRAINT sessions_flow_admission_evidence_check;
