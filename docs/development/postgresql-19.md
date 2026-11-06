---
title: PostgreSQL 19 operations
status: active
created: 2026-10-13
updated: 2026-11-06
summary: Operating mike on PostgreSQL 19 — the major-version requirement, the pgvector image, sqlx nullability under PostgreSQL 19 plans, and parallel autovacuum.
---

# PostgreSQL 19 operations

Mike requires major version 19 for setup, runtime connections, migrations and
backup/restore. Capture uses PostgreSQL 19rc1 and pgvector 0.8.7. All fresh Docker
installations build `scripts/capture-db/Dockerfile.pg19`; installed binaries embed
the same file. Older clusters need a separate major upgrade before running mike.
The historical `upgrade-to-19.sh` intentionally still names its source version.

SQLx can infer non-null results for outer-joined columns under PostgreSQL 19
plans. Annotate nullable query results explicitly with `AS "column?"`, including
when the destination struct already uses `Option`, and regenerate `.sqlx` against
PostgreSQL 19. An `Option` field alone does not prevent a runtime decode error.

## Parallel autovacuum

The image and capture/test Compose configuration set
`autovacuum_max_parallel_workers=4`. Migration
`20261103150000_parallel_autovacuum.sql` assigns four workers to
`requests.deltas` and two each to `requests.messages` and `messages.messages`.
PostgreSQL decides how many workers are available and useful for each vacuum.

On an already running capture server, the server-wide setting can be applied
without restarting. Run the statements separately, outside a transaction:

```sql
ALTER SYSTEM SET autovacuum_max_parallel_workers = 4;
SELECT pg_reload_conf();
```

The Compose setting preserves it across container recreation; command-line
settings override `ALTER SYSTEM`. Check `pg_settings` for the effective value
and `pg_class.reloptions` for the table settings. Observe completed autovacuum
logs and `total_autovacuum_time` before attributing any performance improvement.

## Cost and maintenance statistics

`/api/db-cost` exposes `generic_plan_calls` and `custom_plan_calls` for each
statement over the report window. Counters follow the same reset and eviction
handling as calls and execution time. They reveal plan usage; they do not prove
that a generic plan caused a regression. The current application pool explicitly
requests custom plans.

The `maintenance` object contains cumulative PostgreSQL counters with individual
`stats_reset` timestamps: the 50 tables with most dead tuples (including total
autovacuum milliseconds and autovacuum priority), the 30 largest indexes with
their scan counts, and I/O bytes by backend/object/context. Table and index data
are database-local; `cluster_io` covers the entire server. A null reset time is
unknown, not evidence of a long observation window. A failed maintenance read
leaves statement-cost reporting available with `maintenance: null`.

Use `EXPLAIN (ANALYZE, BUFFERS, IO)` for per-node I/O diagnostics. `ANALYZE`
executes the statement; use an isolated fixture or a suitable transaction for
write queries.

## Large-table migrations and retention

For a large table, add a named `NOT NULL ... NOT VALID` constraint, commit, then
validate in a separate migration so the initial exclusive lock is released:

```sql
ALTER TABLE example ADD CONSTRAINT example_required NOT NULL required_column NOT VALID;
-- A separate migration/transaction:
ALTER TABLE example VALIDATE CONSTRAINT example_required;
```

Always specify `STORED` for indexed generated columns. Use `uuidv7()` for new
database-owned UUID defaults where distributed identity is needed; preserve
existing and provider-issued identifiers and applied migration checksums.

After a substantial retention sweep, measure table and TOAST free space and
ensure disk/WAL headroom before running a targeted command outside a transaction:

```sql
REPACK (CONCURRENTLY) messages.messages;
```

Do not run it on every sweep: the report's measured free space did not justify
a rewrite. Likewise, retain the candidate unused indexes until their reset
times establish at least a month of representative usage. Request-body
compression changes remain a separate application storage project.

See PostgreSQL's [INSERT](https://www.postgresql.org/docs/19/sql-insert.html),
[ALTER TABLE](https://www.postgresql.org/docs/19/sql-altertable.html),
[statistics](https://www.postgresql.org/docs/19/monitoring-stats.html), and
[REPACK](https://www.postgresql.org/docs/19/sql-repack.html) documentation.
