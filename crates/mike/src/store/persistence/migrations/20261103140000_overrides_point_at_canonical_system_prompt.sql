-- Compositions are addressed by overrides.overrides.id and joined to captured
-- requests on system_prompt_id. Capture and create_composition both register a
-- prompt under the lowest registry id for its sha, but two overrides created
-- before that rule (ids 96 and 110, steer-author) point at later duplicates of
-- the same sha that no request carries, so an id join counts 0 where the sha
-- join counted 138. One-shot repair from the fingerprint; the lowest registry
-- id wins a duplicated sha, as in 20261102 request_system_prompts_carry_system_prompt_id.

UPDATE overrides.overrides o
   SET system_prompt_id = canonical.id
  FROM (
        SELECT DISTINCT ON (prompt_sha256) prompt_sha256, id
          FROM system_prompts.system_prompts
         ORDER BY prompt_sha256, id
       ) canonical
 WHERE o.prompt_sha256 = canonical.prompt_sha256
   AND o.system_prompt_id <> canonical.id;
