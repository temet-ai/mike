SELECT t.slug, t.title, t.status, COALESCE(s.tldr, '') AS tldr,
       COALESCE(s.next_up, '') AS next_up, t.parse_error, t.updated_at
  FROM work.threads t
  LEFT JOIN work.thread_state s ON s.thread_id = t.id
 WHERE t.project_id = 1 AND (ARRAY['active','implementing']::text[] IS NULL OR t.status = ANY(ARRAY['active','implementing']::text[]))
 ORDER BY t.updated_at DESC, t.id DESC
 LIMIT 50 OFFSET 0;
