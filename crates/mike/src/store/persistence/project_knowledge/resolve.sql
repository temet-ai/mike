WITH hits(level, project_id) AS (
    SELECT 1, p.id FROM projects.projects p WHERE p.project_uuid = $2
    UNION ALL
    SELECT 2, p.id FROM projects.projects p WHERE p.id = $3
    UNION ALL
    SELECT 3, p.id FROM projects.projects p WHERE p.project_path = $1
    UNION ALL
    SELECT 3, l.project_id FROM projects.locations l WHERE l.root_path = $1
    UNION ALL
    SELECT 4, p.id
      FROM projects.projects p
     WHERE p.merged_into_id IS NULL
       AND (lower(p.custom_name) = lower($1) OR lower(p.display_name) = lower($1))
    UNION ALL
    SELECT 5, p.id
      FROM projects.projects p
     WHERE p.merged_into_id IS NULL
       AND lower(regexp_replace(p.project_path, '^.*/', '')) = lower($1)
),
best AS (
    SELECT min(level) AS level FROM hits
)
SELECT p.id AS "id!", p.project_uuid AS "project_uuid!", p.project_path AS "project_path!",
       COALESCE(p.custom_name, p.display_name) AS "name!"
  FROM projects.projects p
 WHERE p.id IN (SELECT h.project_id FROM hits h JOIN best b ON b.level = h.level)
 ORDER BY p.id
