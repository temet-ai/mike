INSERT INTO launch.tree_changes (run_id, path, change, content_sha)
SELECT 'fleet-x-1', p.path, p.change, p.content_sha
  FROM unnest(ARRAY['a.rs']::text[], ARRAY['modified']::text[], ARRAY['abc']::text[]) AS p(path, change, content_sha)
