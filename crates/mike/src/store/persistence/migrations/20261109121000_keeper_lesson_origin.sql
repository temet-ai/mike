-- memory-keeper supplies lessons inside its own [keeper:<hash>] block. Those
-- rows carry origin 'keeper' and the 'keeper' lane, so they are never counted as
-- the proxy stage's or the mod's deliveries.

ALTER TABLE memory.lesson_injections
    DROP CONSTRAINT IF EXISTS lesson_injections_origin_check;
ALTER TABLE memory.lesson_injections
    ADD CONSTRAINT lesson_injections_origin_check CHECK (origin IN ('proxy', 'mod', 'keeper'));

INSERT INTO memory.lesson_injection_lanes (code) VALUES ('keeper')
ON CONFLICT (code) DO NOTHING;

COMMENT ON COLUMN memory.lesson_injections.origin IS 'proxy: the proxy''s lesson stage added the text to the outgoing request. mod: the mike mod added it as hook context and the client sent it; recorded once per session and lesson, on the first request that carried it. keeper: memory-keeper supplied it inside a [keeper:<hash>] block; recorded once per session and lesson.';
