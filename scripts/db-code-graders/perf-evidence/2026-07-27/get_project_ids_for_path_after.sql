SELECT id FROM projects.projects
WHERE project_path = ANY(ARRAY['/Users/vimes/projects/temet/temet-proxy','projects/temet/temet-proxy'])
ORDER BY array_position(ARRAY['/Users/vimes/projects/temet/temet-proxy','projects/temet/temet-proxy'], project_path)
