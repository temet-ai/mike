-- memory.lesson_injections says which way a lesson reached a session. 'proxy' is
-- the proxy's own lesson stage adding text to the outgoing request; 'mod' is the
-- text the mike mod added as hook context, which the proxy finds already in the
-- request the client sent (lines tagged [lesson:<id>]). A mod row is written once
-- per session and lesson, on the first captured request that carried the tag.
--
-- A constant default adds the column without rewriting the table, and every
-- existing row is a proxy injection.

ALTER TABLE memory.lesson_injections
    ADD COLUMN IF NOT EXISTS origin text NOT NULL DEFAULT 'proxy';

ALTER TABLE memory.lesson_injections
    DROP CONSTRAINT IF EXISTS lesson_injections_origin_check;
ALTER TABLE memory.lesson_injections
    ADD CONSTRAINT lesson_injections_origin_check CHECK (origin IN ('proxy', 'mod'));

COMMENT ON COLUMN memory.lesson_injections.origin IS 'proxy: the proxy''s lesson stage added the text to the outgoing request. mod: the mike mod added it as hook context and the client sent it; recorded once per session and lesson, on the first request that carried it.';
