-- GitHub issue #243 — move worklog thread state into mike, out of
-- artifacts/threads/*/worklog.md and the temet-aide session-start hook that
-- reads it. Full rationale: artifacts/2026-09-03/thread-state-in-mike-design.md.
--
-- The hook this replaces pastes each active thread's "State of play" section
-- into every session verbatim. Measured against captured request bodies,
-- that text stops arriving once a single hook's output passes ~8KB (the
-- thread text was 10,516 characters and, before a same-day prune, 219,416).
-- Checkpoints there also append rather than replace, so the present tense
-- gets buried under its own history — one thread carries ten "Earlier
-- (updated ...)" paragraphs, another reached 389 lines in one section.
--
-- Four tables, in the new `work` schema (not `mailbox` — mailbox.threads
-- already means a conversation thread between sessions; a worklog thread is
-- a different thing and must not borrow that name).
--
--   work.threads               One row per worklog thread: identity and
--                               metadata (slug, project, title, status,
--                               branch, worktree), plus the ingestion-honesty
--                               columns below.
--
--   work.thread_state          The present tense. Exactly one row per
--                               thread, enforced structurally by making
--                               thread_id the primary key rather than a
--                               separate surrogate id — so there is no way
--                               to insert a second row for the same thread
--                               by mistake. REPLACED on every checkpoint,
--                               never appended to: that replace-only
--                               property is the entire fix for the
--                               accumulation problem above. Four separate
--                               text columns rather than one blob, so a
--                               reader gets four short answers instead of
--                               one long section. Carries its own embedding
--                               so a session can be handed the thread state
--                               relevant to what it is doing, not every
--                               active thread's full text — see EMBEDDINGS
--                               below.
--
--   work.thread_acceptance_criteria  One row per thread, the same
--                               ingestion-honesty shape as work.threads,
--                               holding the thread's `<acceptance-criteria>`
--                               block with its own embedding. See
--                               RECONCILIATION NEEDED below.
--
--   work.thread_log             Append-only history. Nothing is lost by
--                               replacing thread_state — the prior state
--                               moves here as a dated entry before being
--                               overwritten.
--
-- EMBEDDINGS (added after the operator asked, mid-build, for thread state
-- and acceptance criteria to be retrievable by vector similarity rather
-- than only injected as parsed markdown — this is what keeps the injector
-- from reintroducing the size problem this whole design exists to fix: it
-- selects the relevant thread instead of concatenating every active one).
-- work.thread_state.embedding and work.thread_acceptance_criteria.embedding
-- match memory.lessons's column names, types and nullability exactly
-- (embedding vector(768), embedding_model text, embedded_at timestamptz,
-- all three NULL together or all three set together — same
-- `..._embedding_consistent` CHECK shape as `lessons_embedding_consistent`
-- in baseline.sql) and are indexed the same way: HNSW,
-- vector_cosine_ops, m=16, ef_construction=64, partial `WHERE embedding IS
-- NOT NULL` — matching idx_lessons_embedding
-- (20260613000003_embedding_nomic_768.sql). Both memory.lessons and
-- memory.session_embeddings carry an HNSW index despite starting empty, so
-- these do too; an ANN index on zero rows costs nothing to build.
--
-- Deliberately NOT added: a `source_updated_at` column alongside the
-- embedding, the way memory.session_embeddings has one. work.thread_state
-- already has `updated_at`, bumped on every checkpoint replace, and
-- work.thread_acceptance_criteria already has `parsed_at`, bumped on every
-- re-parse — both already mean exactly "when did the source text this
-- embedding would be computed from last change". A second column spelling
-- the same fact under a different name is the "two spellings of the same
-- idea" problem the embedding-columns request itself warned against, so
-- the staleness check an embed worker needs is `embedded_at IS NULL OR
-- embedded_at < updated_at` (thread_state) / `embedded_at < parsed_at`
-- (thread_acceptance_criteria), not a new column.
--
-- RECONCILIATION NEEDED: steering.thread_acceptance_criteria
-- (20260805300003_thread_acceptance_criteria.sql) already holds
-- thread_slug/criteria_text/source_path/parsed_at/parse_error, with no
-- embedding, read by the false-done judge bridge in
-- crates/mike/src/steerer/fleet.rs. This migration does not touch that
-- table — it is in active use by a different subsystem and out of this
-- change's scope. work.thread_acceptance_criteria below is a second,
-- separate table with the same shape plus an embedding, keyed on
-- work.threads(id) instead of a bare thread_slug. Two tables now hold the
-- same worklog section under different keys; somebody needs to decide
-- deliberately whether the steerer's bridge should read from work.* instead
-- (removing the duplication) or the two stay independent for a real reason,
-- rather than this drifting into two silently-diverging copies.
--
-- INGESTION HONESTY: source_path / parsed_at / parse_error on work.threads
-- follow the precedent already in steering.thread_acceptance_criteria
-- (20260805300003_thread_acceptance_criteria.sql) — a worklog that exists
-- but fails to parse gets a row with parse_error set, not silence. The same
-- CHECK shape applies: a row must carry either a title or a parse_error,
-- never neither (a totally empty "successful" parse is worse than a
-- recorded failure).
--
-- WHY status/title/branch/worktree ARE NULLABLE: read against all 96 real
-- worklog files under artifacts/threads/ and docs/threads/ before writing
-- this migration. `branch:` is absent from 32 of them, `title:` from 5,
-- `worktree:` is free text with over a dozen distinct spellings for "none"
-- (`null`, `none`, `(none)`, `(none — in-repo)`, `~`, an unset value), and
-- one file (rust-toolchain-latest) has no `status:` line at all. Forcing
-- these NOT NULL would mean fabricating a value ingestion never actually
-- read, which is exactly the dishonesty this design exists to avoid.
--
-- WHY last_session_uuid / thread_log.session_uuid ARE NOT FOREIGN KEYS:
-- these are the session that last touched a thread, parsed out of frontmatter
-- like `last_session: 2026-08-16 (session 83789fd8-...) @ a54e5cba` or a
-- `## Session log` bullet. That session may predate this project's capture,
-- or have run against a repo without the proxy in front of it, so it will
-- sometimes have no row in sessions.sessions. A hard FK would make an
-- otherwise-valid parse fail the whole row insert on an unrelated column —
-- the same silent-loss failure mode the source_path/parse_error columns
-- exist to prevent. This mirrors the already-shipped precedent in
-- memory.session_knowledge.related_session_ids (20260901000001_session_
-- embeddings.sql): a uuid kept as a soft, display-only reference on purpose.
-- Whoever builds the ingestion worker (design doc step 2) should still
-- resolve against sessions.sessions where possible; the schema does not
-- require it to.
--
-- WHY thread_log HAS AN `ord` COLUMN NOT ASKED FOR IN THE DESIGN DOC: the
-- ingestion worker "upserts" (design doc, Ingestion) — it re-parses the
-- whole worklog file and re-runs on every pass, not once. Without a stable
-- per-entry key, re-ingestion has no way to tell "this checkpoint was
-- already recorded" from "this is a new one", and thread_log is meant to
-- stay append-only even when a future direct-write path (not just markdown
-- parsing) adds rows. A UNIQUE constraint on the visible columns alone does
-- not work here: session_uuid and commit_sha are both nullable, and SQL
-- treats NULL as distinct from NULL in a UNIQUE constraint, so two
-- checkpoints on the same day with neither field set would not collide and
-- would duplicate on every re-ingestion. `ord` is the entry's 1-based
-- position within the source file's `## Session log`, giving
-- `UNIQUE (thread_id, ord)` as a real key for `ON CONFLICT DO UPDATE`. Same
-- pattern as the existing `memory.acceptance_criteria` table
-- (`UNIQUE (map_id, ord)`, 20260820000002_acceptance_criteria_maps.sql).
--
-- Idempotent (IF NOT EXISTS throughout) per house migration style.

CREATE SCHEMA IF NOT EXISTS work;

-- ════════════════════════════════════════════════════════════════════
-- work.threads
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS work.threads (
    id                 bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    project_id         bigint      NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    slug               text        NOT NULL,
    title              text        NULL,
    -- Closed set as observed in production worklog frontmatter plus the two
    -- values named in issue #243 that have not appeared there yet
    -- ('implementing', 'review'). NULL is allowed: one real worklog
    -- (rust-toolchain-latest) has no status line at all.
    status             text        NULL,
    branch             text        NULL,
    worktree           text        NULL,
    -- Soft reference to sessions.sessions.session_uuid — see the header
    -- comment for why this is not a foreign key.
    last_session_uuid  uuid        NULL,
    -- Ingestion honesty, precedent: steering.thread_acceptance_criteria.
    source_path        text        NOT NULL,
    parsed_at          timestamptz NOT NULL DEFAULT now(),
    parse_error        text        NULL,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT threads_slug_not_blank
        CHECK (length(trim(slug)) > 0),
    CONSTRAINT threads_status_check
        CHECK (status IN ('active', 'implementing', 'review', 'parked', 'done', 'complete', 'in-progress')),
    CONSTRAINT threads_has_title_or_error
        CHECK (title IS NOT NULL OR parse_error IS NOT NULL),
    CONSTRAINT threads_project_slug_uniq
        UNIQUE (project_id, slug)
);

COMMENT ON TABLE work.threads IS
    'One row per worklog thread (issue #243) — identity and metadata parsed from artifacts/threads/*/worklog.md '
    'or docs/threads/*/worklog.md. source_path/parsed_at/parse_error record ingestion honesty: a worklog that '
    'fails to parse still gets a row rather than vanishing.';
COMMENT ON COLUMN work.threads.parse_error IS
    'Non-NULL when the worklog was read but its fields could not be fully extracted; the row still exists so the failure is visible, not swallowed.';
COMMENT ON COLUMN work.threads.last_session_uuid IS
    'Soft reference to sessions.sessions.session_uuid, deliberately not a foreign key — see migration header comment.';

-- "Active threads for one project, newest first" (ProjectWorkPanel) plus the
-- required index on the project_id foreign key — one composite index serves
-- both. Status filtering (whichever statuses the panel treats as "active")
-- happens as a filter step after this index scan; thread counts per project
-- are in the dozens, not a scale where that costs anything measurable.
CREATE INDEX IF NOT EXISTS idx_work_threads_project_updated
    ON work.threads (project_id, updated_at DESC);

-- ════════════════════════════════════════════════════════════════════
-- work.thread_state — present tense, replaced on every checkpoint
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS work.thread_state (
    -- thread_id IS the primary key, not a separate surrogate id: this is
    -- what makes "exactly one row per thread" a structural fact rather than
    -- an application convention that a bug can violate.
    thread_id       bigint      PRIMARY KEY REFERENCES work.threads(id) ON DELETE CASCADE,
    where_we_are    text        NOT NULL DEFAULT '',
    next_up         text        NOT NULL DEFAULT '',
    blockers        text        NOT NULL DEFAULT '',
    open_questions  text        NOT NULL DEFAULT '',
    -- Doubles as the embedding staleness marker (see EMBEDDINGS in the
    -- migration header) — no separate source_updated_at column.
    updated_at      timestamptz NOT NULL DEFAULT now(),
    -- Vector over the four text columns above, same shape as
    -- memory.lessons.embedding (see EMBEDDINGS in the migration header).
    -- NULL until an embed worker computes it.
    embedding       vector(768),
    embedding_model text,
    embedded_at     timestamptz,
    CONSTRAINT thread_state_embedding_consistent
        CHECK ((((embedding IS NULL) = (embedded_at IS NULL)) AND ((embedding IS NULL) = (embedding_model IS NULL))))
);

COMMENT ON TABLE work.thread_state IS
    'The present tense for one thread (issue #243), one row per thread, REPLACED whole on every checkpoint — '
    'never appended to. Four short answers instead of one accumulating section; history moves to work.thread_log '
    'before being overwritten here. embedding lets a session be handed the thread(s) relevant to what it is doing '
    'rather than every active thread concatenated.';

-- No project_id or status filtering happens against this table directly —
-- every read joins in from work.threads by thread_id, which the primary
-- key already serves as an index. No further index needed at this size.

-- Relevance retrieval for the session-load injector (design doc "Session
-- load" step). Empty at migration time; built now anyway because
-- memory.lessons and memory.session_embeddings both do the same on their
-- vector columns (idx_lessons_embedding, idx_session_embeddings_hnsw) —
-- an ANN index on zero rows costs nothing and this table's whole point is
-- to be read by similarity search once the injector ships.
CREATE INDEX IF NOT EXISTS idx_work_thread_state_embedding_hnsw
    ON work.thread_state USING hnsw (embedding vector_cosine_ops)
    WITH (m = '16', ef_construction = '64')
    WHERE (embedding IS NOT NULL);

-- ════════════════════════════════════════════════════════════════════
-- work.thread_acceptance_criteria — one row per thread, criteria + embedding
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS work.thread_acceptance_criteria (
    -- Same one-row-per-thread structural enforcement as thread_state.
    thread_id       bigint      PRIMARY KEY REFERENCES work.threads(id) ON DELETE CASCADE,
    -- Same ingestion-honesty shape as steering.thread_acceptance_criteria —
    -- see RECONCILIATION NEEDED in the migration header for why this is a
    -- second table rather than an ALTER of that one.
    criteria_text   text        NULL,
    source_path     text        NOT NULL,
    parsed_at       timestamptz NOT NULL DEFAULT now(),
    parse_error     text        NULL,
    -- Same embedding shape as work.thread_state / memory.lessons — see
    -- EMBEDDINGS in the migration header. parsed_at is this table's
    -- staleness marker; no separate source_updated_at column.
    embedding       vector(768),
    embedding_model text,
    embedded_at     timestamptz,
    CONSTRAINT thread_acceptance_criteria_has_text_or_error
        CHECK (criteria_text IS NOT NULL OR parse_error IS NOT NULL),
    CONSTRAINT thread_acceptance_criteria_embedding_consistent
        CHECK ((((embedding IS NULL) = (embedded_at IS NULL)) AND ((embedding IS NULL) = (embedding_model IS NULL))))
);

COMMENT ON TABLE work.thread_acceptance_criteria IS
    'Parsed <acceptance-criteria> block from a thread''s worklog (issue #243), one row per thread, with an '
    'embedding for similarity retrieval. Same ingestion-honesty shape as steering.thread_acceptance_criteria '
    '(that table is untouched, in active use by the false-done judge bridge — see RECONCILIATION NEEDED in the '
    'migration header).';
COMMENT ON COLUMN work.thread_acceptance_criteria.parse_error IS
    'Non-NULL when the worklog was read but no acceptance-criteria block could be extracted; the row still exists so the failure is visible, not swallowed.';

CREATE INDEX IF NOT EXISTS idx_work_thread_acceptance_criteria_embedding_hnsw
    ON work.thread_acceptance_criteria USING hnsw (embedding vector_cosine_ops)
    WITH (m = '16', ef_construction = '64')
    WHERE (embedding IS NOT NULL);

-- ════════════════════════════════════════════════════════════════════
-- work.thread_log — append-only history
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS work.thread_log (
    id            bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    thread_id     bigint      NOT NULL REFERENCES work.threads(id) ON DELETE CASCADE,
    -- 1-based position within the source worklog's "## Session log" section.
    -- Gives re-ingestion a stable key for ON CONFLICT DO UPDATE — see the
    -- migration header comment for why entry_date/session_uuid/commit_sha
    -- alone cannot serve as one.
    ord           integer     NOT NULL,
    entry_date    date        NOT NULL,
    -- Soft reference to sessions.sessions.session_uuid — see the migration
    -- header comment. Nullable: not every log entry names a session (a
    -- "thread sweep" status-review entry, for example, has none).
    session_uuid  uuid        NULL,
    -- Display metadata only, never indexed or used as a lookup key — see
    -- the project-wide rule against indexing content hashes/shas.
    commit_sha    text        NULL,
    summary       text        NOT NULL,
    -- When this row was ingested, distinct from entry_date (when the
    -- checkpoint the row describes actually happened).
    created_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT thread_log_ord_positive
        CHECK (ord >= 1),
    CONSTRAINT thread_log_summary_not_blank
        CHECK (length(trim(summary)) > 0),
    CONSTRAINT thread_log_thread_ord_uniq
        UNIQUE (thread_id, ord)
);

COMMENT ON TABLE work.thread_log IS
    'Append-only checkpoint history for a thread (issue #243). Never edited or bulk-replaced: work.thread_state '
    'is what gets replaced, and the state it replaces moves here first, so nothing is lost.';
COMMENT ON COLUMN work.thread_log.session_uuid IS
    'Soft reference to sessions.sessions.session_uuid, deliberately not a foreign key — see migration header comment.';

-- "One thread's log, newest first" (thread detail view / GET
-- /api/threads/{slug}). thread_id leads, so this also serves as the
-- required index on the thread_id foreign key; UNIQUE (thread_id, ord)
-- above additionally covers it but does not sort by the reader-meaningful
-- entry_date.
CREATE INDEX IF NOT EXISTS idx_work_thread_log_thread_newest
    ON work.thread_log (thread_id, entry_date DESC, ord DESC);

-- ════════════════════════════════════════════════════════════════════
-- Retrieval: a TLDR that is injected, and links to the sessions that
-- hold the detail
-- ════════════════════════════════════════════════════════════════════
--
-- OPERATOR DIRECTION (2026-09-03): a new session should be given what is
-- RELEVANT plus a way to carry on — a short TLDR and links to the sessions
-- whose overviews hold the rest. The goal is a corpus that a session
-- navigates, not a pile that every new session inherits.
--
-- That is why `tldr` is length-bounded rather than just conventionally
-- short. The hook this replaces failed by accumulation: its "State of
-- play" had no ceiling, grew past the 8KB point at which Claude Code
-- silently drops a hook's text, and stopped being delivered at all. A
-- column that can grow without limit will grow without limit. The CHECK is
-- the structural guard that keeps the injected unit a pointer instead of a
-- copy; work.thread_state's four fields and work.thread_log still hold the
-- full text for anyone who follows the link.

ALTER TABLE work.thread_state
    ADD COLUMN IF NOT EXISTS tldr text NOT NULL DEFAULT '';

ALTER TABLE work.thread_state
    DROP CONSTRAINT IF EXISTS thread_state_tldr_bounded;
ALTER TABLE work.thread_state
    ADD CONSTRAINT thread_state_tldr_bounded CHECK (length(tldr) <= 600);

COMMENT ON COLUMN work.thread_state.tldr IS
    'The injected unit: a short summary a session reads to decide whether this thread is its business. '
    'Bounded at 600 chars on purpose — the hook this replaces died of unbounded accumulation. Detail lives '
    'in the other columns, in work.thread_log, and behind the work.thread_sessions links.';

-- embedding / embedding_model / embedded_at and the HNSW index are defined
-- above with the table. Only source_updated_at is added here: it follows
-- memory.session_embeddings and lets the ingestion worker tell a stale
-- embedding from a current one without re-reading the worklog text.
ALTER TABLE work.thread_state
    ADD COLUMN IF NOT EXISTS source_updated_at timestamptz NULL;

-- RETRIEVAL NOTE, verified against a scratch Postgres 17 + pgvector 0.8.2:
-- the embedding indexes here are PARTIAL (`WHERE embedding IS NOT NULL`), so a
-- similarity query only uses them when it repeats that predicate. Without
-- `WHERE embedding IS NOT NULL` the planner sorts instead of using the HNSW
-- index and the query degrades silently as rows accumulate. Verified both
-- ways: omitting it planned a Sort, including it planned an Index Scan using
-- idx_work_thread_state_embedding_hnsw.

COMMENT ON COLUMN work.thread_state.source_updated_at IS
    'Worklog mtime at the moment the embedding was taken — lets the worker skip re-embedding text that has not changed.';

-- ════════════════════════════════════════════════════════════════════
-- work.thread_sessions — the corpus edge
-- ════════════════════════════════════════════════════════════════════
--
-- A thread spans many sessions and a session may touch several threads, so
-- this is the many-to-many between them. It is what makes "link to the
-- sessions that have the overviews" resolvable: mike already serves a
-- per-session overview, so a link here reaches real content instead of
-- being copied into the next session's context.
--
-- session_uuid is a soft reference for the same reason last_session_uuid
-- is — see the migration header. A thread's history must survive session
-- rows being aged out.

CREATE TABLE IF NOT EXISTS work.thread_sessions (
    thread_id     bigint      NOT NULL REFERENCES work.threads(id) ON DELETE CASCADE,
    session_uuid  uuid        NOT NULL,
    -- One line on what this session did for the thread. Short by CHECK for
    -- the same reason tldr is: this is a label on a link, not a summary of
    -- the session — the session's own overview is that.
    contribution  text        NOT NULL DEFAULT '',
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (thread_id, session_uuid),
    CONSTRAINT thread_sessions_contribution_bounded
        CHECK (length(contribution) <= 300)
);

COMMENT ON TABLE work.thread_sessions IS
    'Which sessions worked which thread (issue #243) — the corpus edge. A new session is given a TLDR plus '
    'these links rather than the accumulated text, and follows a link to that session overview when it needs '
    'the detail. session_uuid is a soft reference on purpose; see the migration header.';

-- "Which sessions worked this thread, most recent first" — the panel and
-- the injector both read it that way.
CREATE INDEX IF NOT EXISTS idx_work_thread_sessions_recent
    ON work.thread_sessions (thread_id, last_seen_at DESC);

-- "Which threads did this session work" — the reverse lookup a session
-- makes about itself at load time.
CREATE INDEX IF NOT EXISTS idx_work_thread_sessions_by_session
    ON work.thread_sessions (session_uuid);
