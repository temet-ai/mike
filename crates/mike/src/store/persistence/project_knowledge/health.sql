WITH loc AS (
    SELECT l.location_uuid, l.machine_uuid, m.name AS machine_name, l.root_path, l.last_seen
      FROM projects.locations l
      JOIN projects.machines m ON m.machine_uuid = l.machine_uuid
     WHERE l.project_id = $1
),
src AS (
    SELECT s.location_uuid,
           jsonb_agg(jsonb_build_object(
               'source_uuid', s.source_uuid,
               'name', s.name,
               'root_path', s.root_path,
               'generation', s.generation,
               'last_scan_at', s.last_scan_at,
               'last_heartbeat_at', s.last_heartbeat_at,
               'last_failure_at', s.last_failure_at,
               'last_failure', s.last_failure,
               'last_failure_generation', s.last_failure_generation
           ) ORDER BY s.name, s.source_uuid) AS sources
      FROM memory.knowledge_sources s
      JOIN loc ON loc.location_uuid = s.location_uuid
     GROUP BY s.location_uuid
),
total AS (
    SELECT count(*) AS n
      FROM memory.lesson_file_changes
     WHERE project_id = $1
),
delivered AS (
    SELECT r.location_uuid, count(*) AS n
      FROM loc
      JOIN memory.lesson_file_receipts r ON r.location_uuid = loc.location_uuid
      JOIN memory.lesson_file_changes c ON c.lesson_uuid = r.lesson_uuid AND c.project_id = $1
     WHERE r.version >= c.version
     GROUP BY r.location_uuid
),
conflicted AS (
    SELECT k.location_uuid, count(*) AS n
      FROM memory.lesson_file_conflicts k
      JOIN loc ON loc.location_uuid = k.location_uuid
      JOIN memory.lesson_file_changes c ON c.lesson_uuid = k.lesson_uuid AND c.project_id = $1
     GROUP BY k.location_uuid
),
per_location AS (
    SELECT loc.machine_name, loc.root_path, loc.location_uuid,
           jsonb_build_object(
               'location', jsonb_build_object(
                   'location_uuid', loc.location_uuid,
                   'machine_uuid', loc.machine_uuid,
                   'machine_name', loc.machine_name,
                   'root_path', loc.root_path,
                   'last_seen', loc.last_seen),
               'sources', COALESCE(src.sources, '[]'::jsonb),
               'lessons', jsonb_build_object(
                   'pending', total.n - COALESCE(delivered.n, 0),
                   'conflicted', COALESCE(conflicted.n, 0))
           ) AS entry
      FROM loc
      LEFT JOIN src ON src.location_uuid = loc.location_uuid
      CROSS JOIN total
      LEFT JOIN delivered ON delivered.location_uuid = loc.location_uuid
      LEFT JOIN conflicted ON conflicted.location_uuid = loc.location_uuid
),
all_locations AS (
    SELECT jsonb_agg(entry ORDER BY machine_name, root_path, location_uuid) AS entries
      FROM per_location
)
SELECT jsonb_build_object(
           'project_uuid', p.project_uuid,
           'project_path', p.project_path,
           'as_of', now(),
           'locations', COALESCE(a.entries, '[]'::jsonb)
       ) AS "health!"
  FROM projects.projects p
 CROSS JOIN all_locations a
 WHERE p.id = $1
