WITH live AS (
    SELECT project_id, folder, max(lesson_watermark) AS watermark
      FROM memory.skill_installs
     WHERE retired_at IS NULL
     GROUP BY project_id, folder
), topiced AS (
    SELECT l.id, l.project_id, l.agent_id, l.topic, pl.code AS placement,
           coalesce(nullif(left(btrim(regexp_replace(lower(l.topic), '[^a-z0-9]+', '-', 'g'), '-'), 40), ''), 'project') AS topic_slug,
           coalesce(nullif(left(btrim(regexp_replace(lower(coalesce(nullif(a.name, ''), l.agent_id::text)), '[^a-z0-9]+', '-', 'g'), '-'), 40), ''), 'project') AS agent_slug
      FROM memory.lessons l
      LEFT JOIN memory.lesson_placements pl ON pl.id = l.placement_id
      LEFT JOIN agents.agents a ON a.id = l.agent_id
     WHERE l.archived_at IS NULL AND l.topic IS NOT NULL
)
SELECT t.id
  FROM topiced t
 WHERE t.placement IN ('rule', 'agent', 'nowhere')
UNION
SELECT t.id
  FROM topiced t
  JOIN live ON live.project_id = t.project_id
 WHERE (t.placement IS NULL OR t.placement = 'skill')
   AND t.topic NOT LIKE 'shared:%'
   AND t.topic NOT LIKE 'verifier:%'
   AND t.id <= live.watermark
   AND (live.folder = t.topic_slug
        OR (t.agent_id IS NOT NULL
            AND live.folder = left(t.topic_slug || '-' || t.agent_slug, 63)))
