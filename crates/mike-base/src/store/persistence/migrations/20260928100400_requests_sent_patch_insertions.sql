-- Envelope patches use half-open byte ranges. Trimming a shared prefix and
-- suffix leaves start = end for a pure insertion; reconstruction already
-- accepts that range. Keep negative and reversed ranges invalid, and leave
-- requests_sent_patch_all_or_none unchanged.
--
-- Keep the existing NOT VALID state: enforce new writes without scanning the
-- capture table. sqlx runs the constraint replacement in one transaction.
SET LOCAL lock_timeout = '5s';

ALTER TABLE requests.requests
    DROP CONSTRAINT IF EXISTS requests_sent_patch_range,
    ADD CONSTRAINT requests_sent_patch_range CHECK (
        sent_patch_start IS NULL
        OR (sent_patch_start >= 0 AND sent_patch_end >= sent_patch_start)
    ) NOT VALID;
