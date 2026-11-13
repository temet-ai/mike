-- Session identity, part 2: the notebook. Operator ask (2026-09-20): every
-- session needs a place to keep its own notes, findings and tasks as
-- free-form JSON, and the point is retrieval -- "what did the observer
-- sessions find" must be one query.
--
-- Row per entry, not one JSON blob per session. The operator asked for
-- a "json structure in a table they can edit as they need" -- that is
-- `data`, the free-form payload each session shapes for itself. The
-- surrounding columns exist so the *retrieval* question works: ordering,
-- filtering by kind/status/tag and paging over a single blob column would
-- mean `jsonb_array_elements` on every read, which throws away every index
-- this table can otherwise carry.
--
-- `kind` and `status` are free text, open sets, no CHECK -- same reasoning
-- as sessions.session_goals.role (20260713140000): the vocabulary is
-- declared by the sessions that use it, not fixed by the schema.
-- Conventional kinds: note, finding, task, decision, question.
-- Conventional task statuses: open, in_progress, blocked, done.
--
-- No project_id column. Project identity lives on sessions.sessions and is
-- repointed by heal_project_assignment / assign_session_project; a
-- denormalised copy here would drift the moment a session's project is
-- reassigned after the note was written. A project-scoped query joins
-- through sessions.sessions instead (fetch_project_notes).
--
-- note_uuid is the stable external handle the REST and MCP surface uses;
-- the bigint id never leaves the store layer (matches message_uuid /
-- session_uuid elsewhere in this schema).

CREATE TABLE IF NOT EXISTS workspace.session_notes (
    id         bigserial   PRIMARY KEY,
    note_uuid  uuid        NOT NULL DEFAULT gen_random_uuid(),
    session_id bigint      NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    kind       text        NOT NULL DEFAULT 'note',
    title      text        NOT NULL,
    body       text        NOT NULL DEFAULT '',
    status     text,
    tags       text[]      NOT NULL DEFAULT '{}',
    data       jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE workspace.session_notes IS
    'A session''s own notebook: notes, findings, tasks and decisions it records for itself and for peers to retrieve. One row per entry (never a JSON array blob) so ordering, kind/status/tag filtering and paging stay indexed reads. No project_id -- project identity lives on sessions.sessions and a copy here would drift on reassignment; cross-session queries (fetch_project_notes) join through sessions.sessions.';
COMMENT ON COLUMN workspace.session_notes.note_uuid IS
    'Stable external handle for the REST and MCP surface. The bigint id never leaves the store layer.';
COMMENT ON COLUMN workspace.session_notes.kind IS
    'Free-text, open set, no CHECK -- same reasoning as sessions.session_goals.role. Conventional values: note, finding, task, decision, question.';
COMMENT ON COLUMN workspace.session_notes.status IS
    'Free-text, open set, no CHECK. NULL for a kind with no status concept (a plain note or finding). Conventional task values: open, in_progress, blocked, done.';
COMMENT ON COLUMN workspace.session_notes.data IS
    'The free-form JSON payload each session shapes for itself -- genuinely schemaless from the store''s point of view. Anything filtered or sorted on by a real query (kind, status, tag, title/body text) is its own typed column instead; this column is for structure the notebook itself does not need to interpret.';

-- The session-scoped read path (fetch_session_notes, and the FK column
-- itself): a session's own notes newest-first.
CREATE INDEX IF NOT EXISTS idx_session_notes_session_created
    ON workspace.session_notes (session_id, created_at DESC);

-- note_uuid is the external handle every read/update/delete resolves by.
CREATE UNIQUE INDEX IF NOT EXISTS idx_session_notes_note_uuid
    ON workspace.session_notes (note_uuid);

-- Tag filtering (fetch_project_notes' `tag` param) is array containment
-- (tags @> ARRAY[$1]), which a GIN index on the array column accelerates;
-- a plain btree cannot serve `@>` at all.
CREATE INDEX IF NOT EXISTS idx_session_notes_tags
    ON workspace.session_notes USING gin (tags);

-- jsonb_path_ops (not the default jsonb_ops): smaller index, faster
-- containment (`data @> '...'`) lookups, at the cost of not supporting the
-- `?`/`?|`/`?&` key-existence operators this table has no current query for.
CREATE INDEX IF NOT EXISTS idx_session_notes_data
    ON workspace.session_notes USING gin (data jsonb_path_ops);
