-- no-transaction
-- The file-writes listing and its count filter tools.uses with file_path ILIKE '%text%' over every write row.
-- The b-tree on file_path cannot serve a leading wildcard, so each call read the whole index and fetched the
-- heap row of every entry (about 270 ms). A trigram GIN index over exactly the rows those queries read (write
-- operations with a path) answers the pattern directly. CONCURRENTLY does not block writers; a migration file
-- with CREATE INDEX CONCURRENTLY holds that one statement, because several statements sent together run as one
-- transaction.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tool_uses_write_file_path_trgm
    ON tools.uses USING gin (file_path gin_trgm_ops)
    WHERE operation = 'write' AND file_path IS NOT NULL;
