-- GitHub issue #68, step 1 (the compaction plan's own "irreversible if
-- skipped" step): persist request_body_size as a real column WHILE
-- request_body still exists on every row, so a future compaction pass
-- (nulling request_body on aged, non-final-per-session rows) does not
-- destroy the only source this value is currently computed from
-- (`byte_size(req.request_body.as_ref())`, live-computed at
-- crates/mike/src/api/handlers.rs:637 on every read today).
--
-- octet_length() on a TOASTed bytea reads the TOAST pointer's stored
-- length, not the detoasted value -- confirmed cheap on the live
-- 119GB requests.requests table: EXPLAIN (ANALYZE, BUFFERS) on 1000
-- rows measured 6.9ms, no detoast I/O. NOT NULL with a 0 default for
-- an absent body matches byte_size()'s existing semantics exactly (an
-- absent body and a zero-length body are not currently distinguished
-- on the wire, so this column doesn't invent a new distinction either).
--
-- Backfilled in the same migration (not expand-only across two
-- releases): this table has no concurrent-writer constraint here
-- (migrations apply at startup before the proxy binds its ports; see
-- the covering-index migrations for the established precedent of this
-- table taking non-CONCURRENTLY DDL), and the whole point of this step
-- is closing the "before bodies are gone" window as early as possible.

ALTER TABLE requests.requests ADD COLUMN IF NOT EXISTS request_body_size integer;

UPDATE requests.requests
   SET request_body_size = COALESCE(octet_length(request_body), 0)
 WHERE request_body_size IS NULL;

ALTER TABLE requests.requests ALTER COLUMN request_body_size SET NOT NULL;

COMMENT ON COLUMN requests.requests.request_body_size IS
    'Byte length of request_body, 0 when absent (matches the historical '
    'byte_size() live-compute semantics). Populated at INSERT time going '
    'forward (insert_request_initial); this migration backfills every '
    'existing row from octet_length(request_body) while that column '
    'still holds every row''s bytes. Read this column instead of '
    'computing from request_body once request_body is nulled by a '
    'future compaction pass (GitHub issue #68).';
