SELECT project_id, array_agg(alias_path ORDER BY alias_path) AS merged_paths
FROM projects.project_aliases
GROUP BY project_id
