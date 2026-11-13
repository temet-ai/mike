SELECT a.id, a.name, a.definition_path, a.description,
        s.code AS "source!",
        a.created_at, a.updated_at,
        COALESCE(comp.composition_count, 0)::bigint AS "composition_count!",
        COALESCE(disp.dispatch_count, 0)::bigint AS "dispatch_count!",
        COALESCE(inst.instance_count, 0)::bigint AS "instance_count!",
        COALESCE(sp.prompt_version_count, 0)::bigint AS "prompt_version_count!",
        disp.last_dispatched_at AS "last_dispatched_at?"
   FROM agents.agents a
   JOIN agents.sources s ON s.id = a.source_id
   LEFT JOIN (
       SELECT agent_id, COUNT(*) AS composition_count
         FROM overrides.overrides
        WHERE agent_id IS NOT NULL
        GROUP BY agent_id
   ) comp ON comp.agent_id = a.id
   LEFT JOIN (
       SELECT ad.agent_id, COUNT(*) AS dispatch_count,
              MAX(tu.created_at) AS last_dispatched_at
         FROM agents.dispatches ad
         JOIN tools.uses tu ON tu.id = ad.tool_use_id
        GROUP BY ad.agent_id
   ) disp ON disp.agent_id = a.id
   LEFT JOIN (
       SELECT agent_id, COUNT(*) AS instance_count
         FROM agents.instances
        WHERE agent_id IS NOT NULL
        GROUP BY agent_id
   ) inst ON inst.agent_id = a.id
   LEFT JOIN (
       SELECT agent_id, COUNT(DISTINCT content_sha256) AS prompt_version_count
         FROM system_prompts.session_prompts
        WHERE agent_id IS NOT NULL
          AND content_sha256 IS NOT NULL
        GROUP BY agent_id
   ) sp ON sp.agent_id = a.id
  WHERE a.id = $1
