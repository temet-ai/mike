-- requests.system_prompts.system_prompt_id has existed since the baseline but the
-- capture write never set it: readers joined system_prompts.system_prompts on
-- prompt_sha256, which fans out when the registry holds two rows for one sha
-- (3,825 rows / 3,461 distinct shas on 2026-11-02). The capture write now
-- carries the registry id forward with RETURNING; this fills the rows captured
-- before it, so every reader can join on the id.
--
-- The backfill updates ~238k rows (2026-11-02). Measured on a copy of the two
-- tables: 5.3 s with the FK checked per row and five indexes maintained, 2.3 s
-- with the FK re-added NOT VALID and validated in one scan, the id index rebuilt
-- after the update, and idx_rsp_prompt_sha256 dropped. Nothing reads
-- requests.system_prompts by prompt_sha256 any more (sha columns are never
-- indexed, .claude/rules/sql-set-based.md), so that index goes for good.
-- Transactional: a failure rolls everything back and a re-run starts clean.

SET LOCAL statement_timeout = 0;
SET LOCAL lock_timeout = '60s';

DROP INDEX IF EXISTS requests.idx_rsp_prompt_sha256;

ALTER TABLE requests.system_prompts
    DROP CONSTRAINT IF EXISTS system_prompts_system_prompt_id_fkey;
DROP INDEX IF EXISTS requests.idx_request_system_prompts_sysprompt_id;

-- One-shot backfill from the fingerprint; the lowest registry id wins a
-- duplicated sha. Live writes carry the id and never take this path.
UPDATE requests.system_prompts rsp
   SET system_prompt_id = sp.id
  FROM (
        SELECT DISTINCT ON (prompt_sha256) prompt_sha256, id
          FROM system_prompts.system_prompts
         ORDER BY prompt_sha256, id
       ) sp
 WHERE rsp.prompt_sha256 = sp.prompt_sha256
   AND rsp.system_prompt_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_request_system_prompts_sysprompt_id
    ON requests.system_prompts USING btree (system_prompt_id);

-- Re-added here rather than inline: the constraint already exists on a
-- populated table, and NOT VALID + VALIDATE checks it in one scan.
ALTER TABLE requests.system_prompts
    ADD CONSTRAINT system_prompts_system_prompt_id_fkey
        FOREIGN KEY (system_prompt_id)
        REFERENCES system_prompts.system_prompts(id)
        NOT VALID;
ALTER TABLE requests.system_prompts
    VALIDATE CONSTRAINT system_prompts_system_prompt_id_fkey;
