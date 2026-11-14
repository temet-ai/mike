INSERT INTO launch.tree_changes (run_id, path, change, content_sha)
SELECT $1, p.path, p.change, p.content_sha
  FROM unnest($2::text[], $3::text[], $4::text[]) AS p(path, change, content_sha)
