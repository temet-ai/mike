-- Admission is optional for historical and unsupported producers. No enrollment
-- default or backfill: only the registration INSERT supplies this evidence.
ALTER TABLE sessions.sessions
    ADD COLUMN IF NOT EXISTS flow_capture_version integer,
    ADD COLUMN IF NOT EXISTS flow_admitted_at timestamptz,
    ADD COLUMN IF NOT EXISTS flow_producer text,
    ADD COLUMN IF NOT EXISTS flow_hook_event_name text,
    ADD COLUMN IF NOT EXISTS flow_start_source text;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint
                   WHERE conrelid = 'sessions.sessions'::regclass
                     AND conname = 'sessions_flow_admission_evidence_check') THEN
        ALTER TABLE sessions.sessions ADD CONSTRAINT sessions_flow_admission_evidence_check
        CHECK (
            (flow_capture_version IS NULL AND flow_admitted_at IS NULL
             AND flow_producer IS NULL AND flow_hook_event_name IS NULL
             AND flow_start_source IS NULL)
            OR
            (flow_capture_version IS NOT NULL AND flow_capture_version BETWEEN 1 AND 65535
             AND flow_admitted_at IS NOT NULL
             AND flow_producer IS NOT NULL AND flow_producer = 'claude_code'
             AND flow_hook_event_name IS NOT NULL AND flow_hook_event_name = 'SessionStart'
             AND flow_start_source IS NOT NULL AND flow_start_source = 'startup')
        ) NOT VALID;
    END IF;
END $$;

ALTER TABLE sessions.sessions VALIDATE CONSTRAINT sessions_flow_admission_evidence_check;

COMMENT ON COLUMN sessions.sessions.flow_admitted_at IS
    'Server observation of admission during the session-creating registration transaction; immutable on registration retries, not producer execution time.';
