-- A lesson can hold for one machine only. machine_uuid NULL means every
-- machine; a set machine_uuid means the lesson is supplied only by the proxy
-- running on that machine. Lessons 2250 and 2280 describe infra alone (its
-- pi-limits rule and its MCP_TOOL_TIMEOUT), so they are scoped to infra where
-- that machine and those lessons exist; on any other database the UPDATE
-- matches nothing.

ALTER TABLE memory.lessons
    ADD COLUMN IF NOT EXISTS machine_uuid uuid REFERENCES projects.machines (machine_uuid);

UPDATE memory.lessons l
   SET machine_uuid = m.machine_uuid
  FROM projects.machines m
 WHERE m.name = 'infra'
   AND l.id IN (2250, 2280)
   AND l.machine_uuid IS NULL;
