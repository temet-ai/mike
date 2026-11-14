SELECT fleet_uuid,
       slug,
       CASE WHEN state = 'done' THEN 'retrospective' ELSE state END AS "state!",
       CASE WHEN definition #>> '{retrospective_job,phase}' IN ('pending', 'launching', 'running', 'cleanup', 'blocked')
            THEN COALESCE(definition #>> '{retrospective_job,source_definition,repo}', definition->>'repo')
            ELSE definition->>'repo' END AS "repo!"
  FROM launch.fleets
 WHERE definition->>'repo' IS NOT NULL
   AND (state <> 'done'
        OR definition #>> '{retrospective_job,phase}' IN ('pending', 'launching', 'running', 'cleanup', 'blocked'))
 ORDER BY created_at, id
