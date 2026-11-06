INSERT INTO agents.aliases (agent_id, alias, kind)
 SELECT 45, 'Perf Check Fixture Persona Never Real', 'persona'
  WHERE NOT EXISTS (
      SELECT 1 FROM agents.agents WHERE LOWER(name) = LOWER('Perf Check Fixture Persona Never Real')
  )
 ON CONFLICT (LOWER(alias)) DO NOTHING
