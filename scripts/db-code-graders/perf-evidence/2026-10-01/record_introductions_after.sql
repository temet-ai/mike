INSERT INTO launch.introductions (again_use_id, run_id, first_use_id)
SELECT p.again_use_id, 'fleet-perf-1', p.first_use_id
  FROM unnest(ARRAY[900001,900002,900003]::bigint[], ARRAY[800001,800002,800003]::bigint[]) AS p(again_use_id, first_use_id)
ON CONFLICT (again_use_id) DO NOTHING
RETURNING again_use_id
