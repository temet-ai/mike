SELECT u.session_id, u.id AS tool_use_id, u.tool_name, u.created_at AS at
  FROM tools.uses u
 WHERE u.file_path = '/mnt/data/projects/temet/temet-proxy/artifacts/threads/judgement-at-work/worklog.md'
   AND u.created_at >= '2026-10-01 00:00:00+00'
   AND u.created_at <= '2026-10-06 00:00:00+00'
   AND u.tool_name IN ('Edit', 'Write', 'MultiEdit')
 ORDER BY u.created_at, u.id
