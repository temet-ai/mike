SELECT pc.id, pc.uuid, pc.prompt_sha256, pc.label, pc.blocks, m.code,
       pc.notes, pc.agent_name, pc.agent_definition_path,
       pc.agent_id, pc.version, pc.created_at, pc.updated_at
FROM overrides.overrides pc
JOIN overrides.modes m ON m.id = pc.mode_id
