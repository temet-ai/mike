INSERT INTO projects.projects
    (project_path, display_name, first_seen, last_seen, ttl_override, ttl_override_set_at)
 VALUES ('/home/perftest-after/work/deep/nested/repo', 'work/deep/nested/repo', NOW(), NOW(), '1 hour'::interval,
         NOW())
 ON CONFLICT (project_path) DO NOTHING;
