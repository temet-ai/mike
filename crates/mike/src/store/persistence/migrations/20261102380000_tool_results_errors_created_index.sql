-- no-transaction
-- The tool-error list pages errors newest first by (created_at, id) with a keyset cursor; this
-- partial index lets it walk errors in that order instead of sorting every error per page.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_tool_results_errors_created
    ON tools.results (created_at, id) WHERE is_error;
