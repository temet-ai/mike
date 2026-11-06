SELECT a.alias_path, p.project_path
FROM projects.project_aliases a
JOIN projects.projects p ON p.id = a.project_id
