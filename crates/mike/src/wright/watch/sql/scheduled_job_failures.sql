WITH latest AS (
    SELECT DISTINCT ON (job_name, machine_uuid)
           id, job_name, machine_uuid, started_at, result_kind, exit_code, consecutive_failures,
           output_tail
      FROM jobs.runs
     WHERE started_at >= now() - interval '7 days'
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
 WHERE consecutive_failures >= 2
 ORDER BY consecutive_failures DESC, resource
 LIMIT 200
