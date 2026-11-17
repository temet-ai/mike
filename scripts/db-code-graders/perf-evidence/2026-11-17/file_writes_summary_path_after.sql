SELECT u.file_path,
                COUNT(DISTINCT r.session_id) AS session_count,
                COUNT(*)                     AS write_count,
                MIN(u.created_at)            AS first_write,
                MAX(u.created_at)            AS last_write FROM tools.uses u
           JOIN requests.requests r ON r.id = u.request_id
      LEFT JOIN projects.projects p ON p.id = r.project_id
          WHERE u.operation = 'write'
            AND u.file_path IS NOT NULL AND u.file_path ILIKE '%' || 'file_writes_01.rs' || '%' GROUP BY u.file_path ORDER BY COUNT(DISTINCT r.session_id) DESC, COUNT(*) DESC, u.file_path OFFSET 0 LIMIT 50
