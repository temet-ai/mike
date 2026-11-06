-- Issue #68 step 2: `messages.messages` is sha-deduplicated and strips
-- `cache_control` before hashing (a per-request breakpoint, not message
-- content -- two turns with the same content but a moved breakpoint must
-- still dedupe). That strip is correct for content identity but it is
-- also the only place the breakpoint's placement existed; nothing
-- persisted where it was. Byte-exact `request_body`/`request_sent`
-- reconstruction (the forced-sequence step this issue tracks) needs it
-- back.
--
-- Lives on `requests.messages` (the per-request bridge, one row per
-- (request_id, ord) for every message -- new-to-lineage or carried
-- forward alike), not `requests.deltas` (one row only for messages new
-- to the lineage). A carried-forward message's cache_control can change
-- between requests with no content change and therefore no new deltas
-- row, so deltas is the wrong grain for this fact; the bridge already
-- covers every (request, message) pair unconditionally.
--
-- Nullable, no default, metadata-only ADD COLUMN -- mirrors the
-- 20260721010000 sent_patch precedent. NULL means the wire body carried
-- no cache_control anywhere in this message.
ALTER TABLE requests.messages
    ADD COLUMN IF NOT EXISTS cache_control jsonb;

COMMENT ON COLUMN requests.messages.cache_control IS
    'This request''s cache_control placement on this message, stripped before messages.messages hashes it. JSON array of {"pointer": "<RFC 6901 pointer relative to this message>", "value": <original cache_control object>}, one entry per occurrence (a message can carry more than one content-block breakpoint). NULL when the wire body carried no cache_control on this message.';
