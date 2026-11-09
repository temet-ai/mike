WITH open_threads AS (
    SELECT t.id, t.project_id, t.slug, COALESCE(t.title, t.slug) AS title,
           COALESCE(t.goal, '') AS goal, t.status, t.branch, t.worktree,
           t.last_session_uuid, GREATEST(t.updated_at, s.updated_at) AS checkpoint_at, s.tldr
      FROM work.threads t
      LEFT JOIN work.thread_state s ON s.thread_id = t.id
     WHERE t.status IN ('active', 'implementing', 'review', 'in-progress')
), linked AS (
    SELECT x.thread_id,
           array_agg(DISTINCT x.session_uuid) AS sessions,
           array_agg(DISTINCT ss.cwd) FILTER (WHERE ss.cwd IS NOT NULL) AS dirs
      FROM work.thread_sessions x
      JOIN open_threads o ON o.id = x.thread_id
      LEFT JOIN sessions.sessions ss ON ss.session_uuid = x.session_uuid
     GROUP BY x.thread_id
)
SELECT o.id, o.project_id, o.slug, o.title, o.goal, o.status, o.branch, o.worktree,
       o.last_session_uuid, o.checkpoint_at, o.tldr,
       COALESCE(l.sessions, '{}'), COALESCE(l.dirs, '{}')
  FROM open_threads o
  LEFT JOIN linked l ON l.thread_id = o.id
 ORDER BY o.project_id, o.slug
