-- no-transaction
-- The knowledge search for session overviews matched to_tsvector('english', overview) on every session of the
-- project, which cost about 50 ms on the 1,800 sessions of the largest project. A GIN index over the same expression
-- reads the stored vectors instead. CONCURRENTLY does not block writers; a migration file with CREATE INDEX
-- CONCURRENTLY holds that one statement, because several statements sent together run as one transaction.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_sessions_overview_text
    ON sessions.sessions USING gin (to_tsvector('english'::regconfig, overview));
