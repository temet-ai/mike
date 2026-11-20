-- scheduled_job_failures judged each (job, machine) pair by its latest run in the last 7 days,
-- so a machine that stopped running jobs kept its last failure open for the whole window:
-- problem 62026 came from machine 0b302364, which ran learning_promote four times on an old
-- binary between 2026-11-19 20:30 and 2026-11-20 00:32 and never again. A machine now counts
-- only while it has run some job within max_machine_silence_hours.
UPDATE wright.checks
   SET query = $q$WITH live_machines AS (
    SELECT DISTINCT machine_uuid
      FROM jobs.runs
     WHERE started_at >= now() - make_interval(hours => ($1->>'max_machine_silence_hours')::int)
),
latest AS (
    SELECT DISTINCT ON (job_name, machine_uuid)
           id, job_name, machine_uuid, started_at, result_kind, exit_code, consecutive_failures,
           output_tail
      FROM jobs.runs
     WHERE started_at >= now() - make_interval(days => ($1->>'window_days')::int)
     ORDER BY job_name, machine_uuid, started_at DESC, id DESC
)
SELECT 'worker'::text AS scope,
       'job:' || job_name || coalesce('@' || machine_uuid::text, '') AS resource,
       jsonb_build_object(
           'job', job_name,
           'machine_uuid', machine_uuid,
           'run_id', id,
           'started_at', started_at,
           'result_kind', result_kind,
           'exit_code', exit_code,
           'consecutive_failures', consecutive_failures,
           'output_tail', output_tail
       ) AS evidence
  FROM latest
 WHERE consecutive_failures >= ($1->>'min_consecutive_failures')::int
   AND (machine_uuid IS NULL OR machine_uuid IN (SELECT machine_uuid FROM live_machines))
 ORDER BY consecutive_failures DESC, resource
 LIMIT 200$q$,
       params = params || '{"max_machine_silence_hours": 24}'::jsonb
 WHERE name = 'scheduled_job_failures';
