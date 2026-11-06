SELECT l.location_uuid
FROM sessions.sessions s
JOIN projects.locations l ON l.machine_uuid = '6f1c5c1e-3a52-4f0b-8a0d-0f8e6a4b9c11'
WHERE s.session_uuid = '1139db41-f48e-4b17-9218-98d8b9d62e5b'
  AND s.location_uuid IS NULL
  AND s.project_resolution <> 'operator'
  AND (rtrim(l.root_path, '/') = rtrim('/mnt/data/projects/temet/temet-proxy-wt-wp05', '/')
       OR starts_with('/mnt/data/projects/temet/temet-proxy-wt-wp05', rtrim(l.root_path, '/') || '/'))
ORDER BY length(l.root_path) DESC, l.location_uuid
LIMIT 1
