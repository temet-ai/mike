INSERT INTO launch.told (run_id, what)
SELECT $1, w
  FROM unnest($2::text[]) AS w
ON CONFLICT (run_id, what) DO NOTHING
