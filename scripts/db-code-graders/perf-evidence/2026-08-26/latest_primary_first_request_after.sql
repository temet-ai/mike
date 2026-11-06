SELECT r.id
   FROM requests.requests r
   JOIN requests.turn_origins tor
     ON tor.id = r.turn_origin_id AND tor.code = 'primary_first'
   JOIN projects.projects p ON p.id = r.project_id
  WHERE ('/Users/vimes/projects/temet/trainer/tui'::text IS NULL OR p.project_path = '/Users/vimes/projects/temet/trainer/tui'::text)
  ORDER BY r.created_at DESC
  LIMIT 1
