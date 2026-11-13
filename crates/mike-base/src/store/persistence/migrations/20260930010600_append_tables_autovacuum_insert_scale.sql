-- Insert-driven autovacuum after 1% new rows on the four big append tables, so the
-- visibility map of the recent tail stays current and index-only scans stop fetching
-- heap pages. Metadata only: takes a brief SHARE UPDATE EXCLUSIVE lock, runs no vacuum.
SET LOCAL lock_timeout = '5s';

ALTER TABLE requests.requests SET (autovacuum_vacuum_insert_scale_factor = 0.01);
ALTER TABLE requests.deltas SET (autovacuum_vacuum_insert_scale_factor = 0.01);
ALTER TABLE requests.messages SET (autovacuum_vacuum_insert_scale_factor = 0.01);
ALTER TABLE messages.messages SET (autovacuum_vacuum_insert_scale_factor = 0.01);
