SELECT id FROM projects.projects WHERE project_uuid = (SELECT project_uuid FROM projects.projects WHERE id = 313870) AND project_path <> '(no project)';
