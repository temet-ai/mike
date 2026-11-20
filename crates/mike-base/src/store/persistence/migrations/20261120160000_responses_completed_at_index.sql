-- no-transaction
-- The unscoped find_text response scan reads the responses that completed in its window. The BRIN
-- index on completed_at cannot narrow that: heap pages are reused, so 22k of the table's pages
-- hold rows more than a day apart and a one-day range matched 31k pages. A btree finds the
-- 2.7k pages that hold a day's responses.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_responses_completed_at ON responses.responses (completed_at);
