ALTER TABLE launch.run_records
    ADD COLUMN IF NOT EXISTS time_to_done_minutes double precision NULL,
    ADD COLUMN IF NOT EXISTS fault_minutes_total double precision NULL,
    ADD COLUMN IF NOT EXISTS tokens_per_accepted_task double precision NULL,
    ADD COLUMN IF NOT EXISTS manual_interventions_total bigint NULL,
    ADD COLUMN IF NOT EXISTS score jsonb NULL;

COMMENT ON COLUMN launch.run_records.time_to_done_minutes IS
    'Minutes from the first seat launch to the last seat close. NULL on records written before run-record-v3.';
COMMENT ON COLUMN launch.run_records.fault_minutes_total IS
    'Minutes lost to fleet faults: the sum of the parts in score.fault_minutes. NULL before run-record-v3.';
COMMENT ON COLUMN launch.run_records.tokens_per_accepted_task IS
    'Input, output, cache-read and cache-write tokens of every request in the run divided by the tasks that had a worker seat. NULL before run-record-v3.';
COMMENT ON COLUMN launch.run_records.manual_interventions_total IS
    'Operator turns typed into seats, hand-started seats and answered operator notes. NULL before run-record-v3.';
COMMENT ON COLUMN launch.run_records.score IS
    'The run-record-v3 explanation document: fault_minutes parts, tokens by seat kind, manual intervention parts and the counts that explain them. JSONB because it is read whole by GET /api/fleets/{fleet}/record and nothing filters on a field inside it; the four numbers it explains are the typed columns beside it.';
