-- scheduled_job_failures judged every job per (job, machine), but a leased job
-- (crates/mike/src/scheduled_jobs/jobs.rs, `leased: true`) runs on whichever machine holds
-- the worker-role lease, and the scheduler keeps its history and failure count across the
-- fleet (history.rs: `if job.leased { None } else { machine }`). Problem 78733 stayed open
-- because decisions_loop failed three times on b2923f57 (run 722, a binary older than the
-- database), then the lease moved and the job succeeded elsewhere; b2923f57 kept running
-- other jobs, so its stale run 722 was re-reported every day until it left the window.
-- A leased job is now judged once, by its latest run on any machine, under resource
-- 'job:<name>'. params.leased_jobs lists the leased jobs; the test
-- scheduled_job_checks_tests::scheduled_job_failures_leased_jobs_param_matches_the_scheduler fails when it drifts
-- from JOBS.
UPDATE wright.checks
   SET query = $q$WITH leased AS (
    SELECT jsonb_array_elements_text(coalesce($1->'leased_jobs', '[]'::jsonb)) AS job_name
),
live_machines AS (
    SELECT DISTINCT machine_uuid
      FROM jobs.runs
     WHERE started_at >= now() - make_interval(hours => ($1->>'max_machine_silence_hours')::int)
),
runs AS (
    SELECT r.id, r.job_name, r.machine_uuid, r.started_at, r.result_kind, r.exit_code,
           r.consecutive_failures, r.output_tail,
           (l.job_name IS NOT NULL) AS leased,
           CASE WHEN l.job_name IS NULL THEN r.machine_uuid END AS run_scope
      FROM jobs.runs r
      LEFT JOIN leased l ON l.job_name = r.job_name
     WHERE r.started_at >= now() - make_interval(days => ($1->>'window_days')::int)
),
latest AS (
    SELECT DISTINCT ON (job_name, run_scope)
           id, job_name, machine_uuid, started_at, result_kind, exit_code, consecutive_failures,
           output_tail, leased
      FROM runs
     ORDER BY job_name, run_scope, started_at DESC, id DESC
)
SELECT 'worker'::text AS scope,
       'job:' || job_name
           || CASE WHEN leased THEN '' ELSE coalesce('@' || machine_uuid::text, '') END AS resource,
       jsonb_build_object(
           'job', job_name,
           'leased', leased,
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
   AND (leased
        OR machine_uuid IS NULL
        OR machine_uuid IN (SELECT machine_uuid FROM live_machines))
 ORDER BY consecutive_failures DESC, resource
 LIMIT 200$q$,
       params = params || '{"leased_jobs": ["decisions_loop", "learning_audit", "lesson_relations", "memory_review", "usage_audit"]}'::jsonb
 WHERE name = 'scheduled_job_failures';
