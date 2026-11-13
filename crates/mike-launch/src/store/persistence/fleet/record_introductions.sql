INSERT INTO launch.introductions (again_use_id, run_id, first_use_id)
SELECT p.again_use_id, $1, p.first_use_id
  FROM unnest($2::bigint[], $3::bigint[]) AS p(again_use_id, first_use_id)
ON CONFLICT (again_use_id) DO NOTHING
RETURNING again_use_id
