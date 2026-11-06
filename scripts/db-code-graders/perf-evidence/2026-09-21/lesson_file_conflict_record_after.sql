INSERT INTO memory.lesson_file_conflicts(location_uuid, lesson_uuid, server_version)
SELECT '05ba052d-5a10-4d60-ad5f-dddb8821c3b3', c.lesson_uuid, c.version
  FROM unnest(ARRAY['98a18513-b194-44b9-8267-a1bd8fd130ca','e806a7d7-bff1-4fc6-bfa0-af5842e5545e']::uuid[]) AS x(lesson_uuid)
  JOIN memory.lesson_file_changes c ON c.lesson_uuid = x.lesson_uuid
  JOIN projects.locations l
    ON l.location_uuid = '05ba052d-5a10-4d60-ad5f-dddb8821c3b3' AND l.project_id = c.project_id
ON CONFLICT(location_uuid, lesson_uuid) DO UPDATE
  SET server_version = EXCLUDED.server_version, detected_at = now()
