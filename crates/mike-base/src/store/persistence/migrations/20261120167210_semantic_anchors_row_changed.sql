-- `proxy.row_changed` for new anchor rows in `memory.semantic_items`, so the
-- semantic worker on every machine learns at once that an anchor may need
-- carrying to its current embedder.
--
-- The worker carried anchors by asking on every tick (every 300 s on each
-- machine) for anchors under another embedder key with no copy under the
-- current one. On 2026-11-22 that statement ran about 66 times an hour across
-- the five machines and returned nothing: anchors are written only by
-- `mike semantic anchors import` and by the carry itself, and every anchor
-- already had its copy. The worker now asks once at start, again when the
-- current embedder changes, and otherwise only after this notice (or a
-- listener resync). Only anchor inserts notify; the turn and directive items
-- the worker writes all day stay silent.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_semantic_anchors()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'semantic_anchors',
        'id',         NEW.id::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_semantic_anchors_row_changed
AFTER INSERT ON memory.semantic_items
FOR EACH ROW
WHEN (NEW.kind = 'anchor')
EXECUTE FUNCTION mike.notify_row_changed_semantic_anchors();
