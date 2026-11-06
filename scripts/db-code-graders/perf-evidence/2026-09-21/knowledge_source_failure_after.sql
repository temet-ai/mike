UPDATE memory.knowledge_sources SET last_failure = 'malformed document design.md', last_failure_at = now(),
       last_failure_generation = generation, last_heartbeat_at = now()
 WHERE source_uuid = '167563b7-5c8c-4eb2-9aea-5cee914baf47'
