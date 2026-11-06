WITH hit AS (
     SELECT id, project_path, array_position(ARRAY['/mnt/data/projects/temet/temet-proxy/.claude/worktrees/x','/mnt/data/projects/temet/temet-proxy','projects/temet/temet-proxy']::text[], project_path) AS rank
       FROM projects.projects
      WHERE project_path = ANY(ARRAY['/mnt/data/projects/temet/temet-proxy/.claude/worktrees/x','/mnt/data/projects/temet/temet-proxy','projects/temet/temet-proxy']::text[])
 )
 SELECT id, project_path, rank, NULL::timestamptz AS last_seen FROM hit
 UNION ALL
 SELECT id, project_path, NULL::int AS rank, last_seen
   FROM projects.projects
  WHERE (false OR (true AND NOT EXISTS (SELECT 1 FROM hit)))
    AND project_path LIKE '%' || '/mnt/data/projects/temet/temet-proxy/.claude/worktrees/x'
