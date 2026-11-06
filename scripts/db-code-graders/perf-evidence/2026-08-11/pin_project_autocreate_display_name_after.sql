INSERT INTO projects.projects
    (project_path, display_name, first_seen, last_seen, pinned, pin_created_at)
 VALUES ('/Users/perftest-after/projects/some/deep/nested/path', 'projects/some/deep/nested/path', NOW(), NOW(), TRUE, NOW())
 ON CONFLICT (project_path) DO NOTHING;
