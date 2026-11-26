WITH leased AS (
    SELECT jsonb_array_elements_text(coalesce('{"window_days": 7, "min_consecutive_failures": 2, "max_machine_silence_hours": 24, "leased_jobs": ["decisions_loop", "learning_audit", "lesson_relations", "memory_review", "usage_audit"]}'::jsonb->'leased_jobs', '[]'::jsonb)) AS job_name
),
live_machines AS (
    SELECT DISTINCT machine_uuid
      FROM jobs.runs
     WHERE started_at >= now() - make_interval(hours => ('{"window_days": 7, "min_consecutive_failures": 2, "max_machine_silence_hours": 24, "leased_jobs": ["decisions_loop", "learning_audit", "lesson_relations", "memory_review", "usage_audit"]}'::jsonb->>'max_machine_silence_hours')::int)
),
runs AS (
    SELECT r.id, r.job_name, r.machine_uuid, r.started_at, r.result_kind, r.exit_code,
           r.consecutive_failures, r.output_tail,
           (l.job_name IS NOT NULL) AS leased,
           CASE WHEN l.job_name IS NULL THEN r.machine_uuid END AS run_scope
      FROM jobs.runs r
      LEFT JOIN leased l ON l.job_name = r.job_name
     WHERE r.started_at >= now() - make_interval(days => ('{"window_days": 7, "min_consecutive_failures": 2, "max_machine_silence_hours": 24, "leased_jobs": ["decisions_loop", "learning_audit", "lesson_relations", "memory_review", "usage_audit"]}'::jsonb->>'window_days')::int)
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
 WHERE consecutive_failures >= ('{"window_days": 7, "min_consecutive_failures": 2, "max_machine_silence_hours": 24, "leased_jobs": ["decisions_loop", "learning_audit", "lesson_relations", "memory_review", "usage_audit"]}'::jsonb->>'min_consecutive_failures')::int
   AND (leased
        OR machine_uuid IS NULL
        OR machine_uuid IN (SELECT machine_uuid FROM live_machines))
 ORDER BY consecutive_failures DESC, resource
 LIMIT 200;
