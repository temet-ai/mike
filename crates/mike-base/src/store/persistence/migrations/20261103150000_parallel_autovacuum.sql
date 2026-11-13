-- PostgreSQL 19 parallel index vacuum. The server worker budget is configured
-- separately by the operator (autovacuum_max_parallel_workers = 4).
SET LOCAL lock_timeout = '5s';
ALTER TABLE requests.deltas SET (autovacuum_parallel_workers = 4);
ALTER TABLE requests.messages SET (autovacuum_parallel_workers = 2);
ALTER TABLE messages.messages SET (autovacuum_parallel_workers = 2);
