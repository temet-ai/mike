WITH skill_topics AS (
    SELECT l.project_id, l.topic, count(*) AS lessons
      FROM memory.lessons l
      JOIN memory.lesson_stages ls
        ON ls.id = l.stage_id AND ls.code IN ('daily', 'long_term')
      JOIN memory.lesson_placements lp
        ON lp.id = l.placement_id AND lp.code = 'skill'
     WHERE l.archived_at IS NULL
       AND l.topic IS NOT NULL
       AND l.topic NOT LIKE 'shared:%'
       AND l.topic NOT LIKE 'verifier:%'
       AND l.project_id IS NOT NULL
     GROUP BY l.project_id, l.topic),
shipped AS (
    SELECT DISTINCT d.project_id, d.topic
      FROM memory.skill_drafts d
     WHERE d.status = 'accepted' AND d.topic IS NOT NULL)
SELECT t.project_id, t.topic, t.lessons, (s.topic IS NOT NULL) AS shipped
  FROM skill_topics t
  LEFT JOIN shipped s ON s.project_id = t.project_id AND s.topic = t.topic
 ORDER BY t.project_id, t.topic
