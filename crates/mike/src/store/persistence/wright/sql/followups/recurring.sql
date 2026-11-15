WITH last_closed AS (
    SELECT check_name, max(closed_at) AS closed_at
      FROM wright.followups
     WHERE kind = 'root_cause' AND closed_at IS NOT NULL AND check_name IS NOT NULL
     GROUP BY check_name
),
counted AS (
    SELECT a.remedy, r.check_name, count(*) AS actions
      FROM wright.actions a
      JOIN wright.remedies r ON r.name = a.remedy
      LEFT JOIN last_closed lc ON lc.check_name = r.check_name
     WHERE a.state IN ('verified', 'ineffective')
       AND a.verified_at >= now() - make_interval(days => $1::int)
       AND a.verified_at > COALESCE(lc.closed_at, '-infinity'::timestamptz)
     GROUP BY a.remedy, r.check_name
    HAVING count(*) >= $2::bigint
),
newest AS (
    SELECT DISTINCT ON (p.check_name)
           p.check_name, p.id AS problem_id, e.id AS episode_id
      FROM wright.problems p
      JOIN wright.episodes e ON e.problem_id = p.id AND e.closed_at IS NULL
     WHERE p.state = 'open'
       AND p.check_name IN (SELECT check_name FROM counted)
     ORDER BY p.check_name, p.id DESC
)
SELECT c.check_name, c.remedy, c.actions,
       n.problem_id AS newest_problem_id, n.episode_id AS newest_episode_id
  FROM counted c
  JOIN newest n ON n.check_name = c.check_name
 WHERE NOT EXISTS (
       SELECT 1 FROM wright.followups f
        WHERE f.kind = 'root_cause' AND f.check_name = c.check_name
          AND f.state IN ('open', 'filed', 'handed_off', 'stuck'))
 ORDER BY c.actions DESC, c.remedy
