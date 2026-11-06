SELECT id FROM projects.projects
 WHERE project_path = ANY(ARRAY['/mnt/data/projects/temet/temet-proxy','projects/temet/temet-proxy'])
 ORDER BY array_position(ARRAY['/mnt/data/projects/temet/temet-proxy','projects/temet/temet-proxy'], project_path)
 LIMIT 1
