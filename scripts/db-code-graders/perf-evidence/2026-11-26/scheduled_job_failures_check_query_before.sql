WITH live_machines AS (
    SELECT DISTINCT machine_uuid
      FROM jobs.runs
     WHERE started_at >= now() - make_interval(hours => ('{"window_days": 7, "min_consecutive_failures": 2, "max_machine_silence_hours": 24, "leased_jobs": ["decisions_loop", "learning_audit", "lesson_relations", "memory_review", "usage_audit"]}'::jsonb->>'max_machine_silence_hours')::int)
),
latest AS (
    SELECT DISTINCT ON (job_name, machine_uuid)
           id, job_name, machine_uuid, started_at, result_kind, exit_code, consecutive_failures,
           output_tail
      FROM jobs.runs
     WHERE started_at >= now() - make_interval(days => ('{"window_days": 7, "min_consecutive_failures": 2, "max_machine_silence_hours": 24, "leased_jobs": ["decisions_loop", "learning_audit", "lesson_relations", "memory_review", "usage_audit"]}'::jsonb->>'window_days')::int)
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
 WHERE consecutive_failures >= ('{"window_days": 7, "min_consecutive_failures": 2, "max_machine_silence_hours": 24, "leased_jobs": ["decisions_loop", "learning_audit", "lesson_relations", "memory_review", "usage_audit"]}'::jsonb->>'min_consecutive_failures')::int
   AND (machine_uuid IS NULL OR machine_uuid IN (SELECT machine_uuid FROM live_machines))
 ORDER BY consecutive_failures DESC, resource
 LIMIT 200;
