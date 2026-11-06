SELECT p.project_path, a.alias_path
FROM projects.project_aliases a
JOIN projects.projects p ON p.id = a.project_id
ORDER BY a.alias_path
