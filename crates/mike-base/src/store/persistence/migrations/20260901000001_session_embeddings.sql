-- Session-level and project-level embedding vectors, plus the session
-- judge's per-session topics/learnings — artifacts/2026-09-01/embedding-
-- graph-plan.md, unit 1 (db-dev). Backs the session and project
-- "embedding graph" panels and the judge's related-sessions context.
--
-- Three tables:
--
--   memory.session_embeddings  One vector per session. Until the session
--                               judge writes a narrative overview, the
--                               vector is the L2-normalised mean of the
--                               session's operator-turn chunk embeddings
--                               (source='centroid'); once an overview
--                               exists, the vector is the overview's own
--                               embedding (source='overview'). Both live
--                               in the same nomic-embed-text-v1.5 768-dim
--                               space, so cosine similarity between a
--                               centroid row and an overview row is still
--                               meaningful.
--
--                               STALENESS RULE (enforced by the embed
--                               worker's sweep query, not by this
--                               migration): a session is due for a
--                               refresh when it has no row here yet and
--                               at least one embedded turn chunk
--                               (missing), or when its row's
--                               source_updated_at is older than
--                               GREATEST(the newest chunk.embedded_at
--                               across its turns, sessions.sessions.
--                               overview_generated_at) (stale).
--
--   memory.project_embeddings  One row per project — the L2-normalised
--                               mean of that project's session vectors.
--                               Recomputed whenever a project's newest
--                               session vector is newer than the
--                               project's own row.
--
--   memory.session_knowledge   One row per judged session — the topics
--                               and durable learnings the session judge
--                               extracted, plus the neighbour sessions it
--                               was shown while writing them. Replaced
--                               (not appended) on every judge run, mirror
--                               of sessions.sessions.overview. This is
--                               NOT memory.lessons: machine-derived
--                               topics/learnings must never be written
--                               into memory.lessons, because
--                               list_injectable_lessons
--                               (lesson_relations.rs) injects every
--                               non-archived lesson stage into future
--                               prompts on the project, and this content
--                               has not been through the operator/agent
--                               lesson-authoring path.
--
-- session_id on session_embeddings/session_knowledge is `uuid`, keyed on
-- sessions.sessions.session_uuid (UNIQUE, not the bigint surrogate `id`
-- every other memory.* table FKs against). This deliberately diverges
-- from memory.operator_turns.session_id / memory.lessons.session_id
-- (both bigint): every caller of this pair of tables — the embed worker
-- sweep, the two embedding-graph REST routes, the session judge, the
-- lesson-writer's related-sessions lookup — already holds the session's
-- UUID (it is the API's and the judge's native session identifier) and
-- would otherwise pay a bigint-resolution join on every call. Postgres
-- allows a FK to target any UNIQUE column, not only the primary key.
--
-- project_embeddings.project_id stays the existing bigint convention
-- (projects.projects.id has no separate UUID identifier).
--
-- Idempotent (IF NOT EXISTS throughout) per house migration style.

-- ════════════════════════════════════════════════════════════════════
-- memory.session_embeddings
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS memory.session_embeddings (
    session_id         uuid          PRIMARY KEY
                                      REFERENCES sessions.sessions(session_uuid) ON DELETE CASCADE,
    embedding          vector(768)   NOT NULL,
    embedding_model    text          NOT NULL,
    -- Closed set: which source produced `embedding`. See the staleness
    -- rule in the header comment.
    source             text          NOT NULL CHECK (source IN ('overview', 'centroid')),
    chunk_count        integer       NOT NULL DEFAULT 0,
    -- Timestamp of the source content this vector was computed from: the
    -- max chunk.embedded_at over the session's turns for a centroid row,
    -- or sessions.sessions.overview_generated_at for an overview row.
    -- Compared against fresh source timestamps to detect staleness.
    source_updated_at  timestamptz   NOT NULL,
    embedded_at        timestamptz   NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_session_embeddings_hnsw
    ON memory.session_embeddings USING hnsw (embedding vector_cosine_ops)
    WITH (m = '16', ef_construction = '64');

COMMENT ON TABLE memory.session_embeddings IS
    'One vector per session with embedded turns: the L2-normalised mean of its turn-chunk '
    'embeddings until the session judge writes a narrative overview, that overview''s own '
    'embedding after (source distinguishes the two). Refreshed by the embed worker sweep '
    '(list_sessions_needing_embedding) and, for the overview case, immediately by the judge '
    'after it writes sessions.sessions.overview. Keyed on session_uuid, not the bigint '
    'surrogate id — every caller already holds the session UUID.';

-- ════════════════════════════════════════════════════════════════════
-- memory.project_embeddings
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS memory.project_embeddings (
    project_id      bigint        PRIMARY KEY
                                   REFERENCES projects.projects(id) ON DELETE CASCADE,
    embedding       vector(768)   NOT NULL,
    embedding_model text          NOT NULL,
    session_count   integer       NOT NULL,
    embedded_at     timestamptz   NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_project_embeddings_hnsw
    ON memory.project_embeddings USING hnsw (embedding vector_cosine_ops)
    WITH (m = '16', ef_construction = '64');

COMMENT ON TABLE memory.project_embeddings IS
    'One row per project: the L2-normalised mean of memory.session_embeddings across the '
    'project''s sessions, with session_count. Recomputed by refresh_project_centroids for any '
    'project whose newest session vector is newer than this row (or that has no row yet).';

-- ════════════════════════════════════════════════════════════════════
-- memory.session_knowledge
-- ════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS memory.session_knowledge (
    session_id           uuid          PRIMARY KEY
                                        REFERENCES sessions.sessions(session_uuid) ON DELETE CASCADE,
    -- Three to five short labels and zero to five durable claims the
    -- session judge extracted. Denormalised text[] rather than a child
    -- table: these are judge-authored display strings shown verbatim on
    -- the session page and in the overview API, never individually
    -- filtered, sorted, or joined on — the same rationale as
    -- memory.operator_turns.tools_used/agents_dispatched/files_touched.
    topics                text[]        NOT NULL DEFAULT '{}',
    learnings             text[]        NOT NULL DEFAULT '{}',
    -- The neighbour sessions the judge was shown while writing this row
    -- (soft references, display-only — same denormalisation rationale as
    -- topics/learnings above; a Postgres array cannot itself carry a FK,
    -- and these are judge-run history, not a relation this schema
    -- enforces).
    related_session_ids  uuid[]        NOT NULL DEFAULT '{}',
    verdict_id            bigint       NULL
                                        REFERENCES session_judges.verdicts(id) ON DELETE SET NULL,
    generated_at           timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_session_knowledge_verdict_id
    ON memory.session_knowledge (verdict_id);

COMMENT ON TABLE memory.session_knowledge IS
    'One row per judged session: the session judge''s extracted topics and durable learnings, '
    'replaced (not appended) on every judge run. This is where session learnings live for '
    'display — never copied into memory.lessons, which is injected into every future prompt '
    'on the project (list_injectable_lessons takes every non-archived stage).';
