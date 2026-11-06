-- Phase 2 / Local-DB Fallback Resync: source-side sync bookkeeping.
--
-- Companion to docs/LOCAL_DB_FALLBACK.md ("Architecture B — Async resync
-- worker"). These two tables are the durable state the local→remote resync
-- worker (built next, in Rust) reads and writes. They are DDL only — no
-- application queries ship in this migration.
--
-- Scope: these are SOURCE-SIDE bookkeeping. They are only meaningful on the
-- LOCAL fallback DB — the side that captured rows while the remote infra DB
-- was unreachable and now needs to push them upstream. On the remote DB (the
-- normal target) and on any DB that never acts as a fallback source, they sit
-- empty and harmless: the worker only writes them on the instance doing the
-- pushing. Because every migration runs on every DB, they exist everywhere;
-- only the local fallback ever populates them.
--
-- Design choices:
--   * sync schema — a new domain schema parallel to config/memory/rules.
--     Keeps all resync-worker bookkeeping out of the captured-data schemas
--     (requests., responses., sessions., …) it reads from.
--
--   * sync.cursor — per-source-table high-water mark of the largest local
--     surrogate id the worker has already pushed to the remote DB. The fact
--     tables are append-only (GENERATED ALWAYS AS IDENTITY ids only grow), so
--     "everything with local id > last_local_id is unsynced" is a correct,
--     resumable, idempotent incremental cursor: a re-run after a crash or
--     restart simply rescans from the persisted high-water mark and never
--     re-pushes a row already below it. table_name is the natural key (e.g.
--     'requests.requests'), so it is the PRIMARY KEY; the row set is tiny
--     (one row per synced fact table) and never joined on, so the PK is the
--     only index needed.
--
--   * sync.id_map — durable local→remote surrogate id remapping. Local and
--     remote DBs assign GENERATED ALWAYS AS IDENTITY ids independently, so a
--     local row's id is meaningless on the remote side. When the worker
--     inserts a local row remotely it records (table_name, local_id) ->
--     remote_id here, and resolves every child FK through this map before
--     inserting the child. Persisting it (rather than holding it in memory
--     per run) is what makes the worker resolve child FKs across separate
--     runs and survive restarts mid-replay, and what makes re-runs idempotent
--     — a row already in the map is already remote, so it is skipped, not
--     re-inserted. (table_name, local_id) is the natural composite key, hence
--     the PRIMARY KEY; that same composite index serves the worker's
--     "is this local id already mapped?" lookups, so no secondary index is
--     needed. Keys here are low-entropy (a short schema-qualified table_name
--     plus a bigint id) — never a content hash — so the PK B-tree has good
--     locality; see .claude/rules/sql-set-based.md.
--
-- Deliberately NOT here: no NOTIFY trigger on the captured/synced tables. A
-- trigger firing per captured request would add write cost on the proxy hot
-- path (.claude/rules/db-performance-evaluation.md, hot tier = 0% tolerance).
-- The worker polls on an interval / wakes on remote-reachability recovery
-- instead. Do not add capture-path triggers to drive this worker.

CREATE SCHEMA IF NOT EXISTS sync;

CREATE TABLE IF NOT EXISTS sync.cursor (
    table_name    text        PRIMARY KEY,
    last_local_id bigint      NOT NULL DEFAULT 0,
    updated_at    timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE sync.cursor IS
    'Source-side per-fact-table high-water mark for the local->remote resync '
    'worker (docs/LOCAL_DB_FALLBACK.md). One row per synced fact table; '
    'last_local_id is the largest local surrogate id already pushed to the '
    'remote DB. Append-only fact ids only grow, so rows with id > last_local_id '
    'are exactly the unsynced ones — incremental, resumable, idempotent. '
    'Only populated on the local fallback DB; empty and harmless elsewhere.';

COMMENT ON COLUMN sync.cursor.table_name IS
    'Schema-qualified source table whose sync progress this row tracks, '
    'e.g. ''requests.requests''. Natural key, hence the PRIMARY KEY.';

COMMENT ON COLUMN sync.cursor.last_local_id IS
    'Largest local surrogate id of this table already pushed to the remote DB. '
    'The worker reads rows with id > last_local_id, then advances this value. '
    'Defaults to 0 so a never-synced table starts from the beginning.';

COMMENT ON COLUMN sync.cursor.updated_at IS
    'Wall-clock time of the last cursor advance for this table (now() on '
    'upsert). Surfaced as sync lag/last-run in the planned status panel.';

CREATE TABLE IF NOT EXISTS sync.id_map (
    table_name text        NOT NULL,
    local_id   bigint      NOT NULL,
    remote_id  bigint      NOT NULL,
    synced_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (table_name, local_id)
);

COMMENT ON TABLE sync.id_map IS
    'Source-side durable local->remote surrogate id remapping for the resync '
    'worker (docs/LOCAL_DB_FALLBACK.md). Each row records that local row '
    '(table_name, local_id) was inserted remotely as remote_id. The worker '
    'resolves child FKs through this map before inserting children, so the '
    'mapping must survive across runs and restarts — hence it is persisted, '
    'not held per-run in memory. Persisting it also makes replay idempotent: '
    'a (table_name, local_id) already present is already remote and is skipped. '
    'Only populated on the local fallback DB; empty and harmless elsewhere.';

COMMENT ON COLUMN sync.id_map.table_name IS
    'Schema-qualified source table the local_id belongs to, e.g. '
    '''requests.requests''. Part of the composite PRIMARY KEY.';

COMMENT ON COLUMN sync.id_map.local_id IS
    'Surrogate id of the row in the LOCAL DB. Part of the composite PRIMARY '
    'KEY; combined with table_name it uniquely identifies a source row.';

COMMENT ON COLUMN sync.id_map.remote_id IS
    'Surrogate id the same row was assigned when inserted into the REMOTE DB. '
    'Child FKs pointing at the local row are rewritten to this value before '
    'the child is inserted remotely.';

COMMENT ON COLUMN sync.id_map.synced_at IS
    'Wall-clock time the local->remote mapping was recorded (now() on insert).';
