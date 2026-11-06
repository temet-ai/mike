SELECT p.id AS project_id, p.project_uuid, p.project_path,
       p.first_seen, p.last_seen, p.pinned, p.ttl_override, p.pin_created_at, p.ttl_override_set_at, p.custom_name
FROM projects.projects p
WHERE p.project_path <> '(no project)' AND p.merged_into_id IS NULL
ORDER BY p.last_seen DESC
