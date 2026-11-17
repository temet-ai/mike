SELECT COUNT(*) FROM (SELECT u.file_path FROM tools.uses u
           JOIN requests.requests r ON r.id = u.request_id
      LEFT JOIN projects.projects p ON p.id = r.project_id
          WHERE u.operation = 'write'
            AND u.file_path IS NOT NULL AND u.file_path ILIKE '%' || 'file_writes_01.rs' || '%' GROUP BY u.file_path) g
