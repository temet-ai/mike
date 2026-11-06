-- Embedding cutover Phase A: vector(384) → vector(768) on embedding columns.
--
-- Pre-condition verified at migration-authoring time against the live DB:
--   memory.operator_turn_chunks: 0 rows
--   memory.lessons (embedding IS NOT NULL): 0 rows
-- The corpus was reset to 0 by the 2026-06-04 squash. This ALTER is safe
-- on empty tables and requires no data migration.
--
-- If either table has rows when this migration runs, it will fail with:
--   ERROR: table "operator_turn_chunks" contains 384-dimensional data but
--          index expects 768 dimensions
-- In that case STOP and report — do not delete data to force the migration.

-- ─── memory.operator_turn_chunks ──────────────────────────────────────────
-- chunk_embedding: vector(384) → vector(768).
-- The existing HNSW index memory.idx_otc_embedding is live (referenced by
-- recall_turns_by_embedding, persistence/mod.rs:3706) and must be rebuilt
-- for the new dimension. Drop schema-qualified, then recreate.
DROP INDEX IF EXISTS memory.idx_otc_embedding;

ALTER TABLE memory.operator_turn_chunks
    ALTER COLUMN chunk_embedding TYPE vector(768);

CREATE INDEX idx_otc_embedding ON memory.operator_turn_chunks
    USING hnsw (chunk_embedding vector_cosine_ops)
    WITH (m = '16', ef_construction = '64');

-- ─── memory.lessons ───────────────────────────────────────────────────────
-- embedding: vector(384) nullable → vector(768) nullable.
-- The HNSW index memory.idx_lessons_embedding covers only non-NULL rows.
-- Rebuild after ALTER.
DROP INDEX IF EXISTS memory.idx_lessons_embedding;

ALTER TABLE memory.lessons
    ALTER COLUMN embedding TYPE vector(768);

CREATE INDEX idx_lessons_embedding ON memory.lessons
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = '16', ef_construction = '64')
    WHERE (embedding IS NOT NULL);

-- ─── Model alignment note ─────────────────────────────────────────────────
-- After this migration the schema requires 768-dim vectors.
-- Embedding model: nomic-ai/nomic-embed-text-v1.5 (768-dim), served by
-- Infinity at http://127.0.0.1:7997. Use nomic task prefixes:
--   search_document: for corpus chunks (operator_turn_chunks, lessons)
--   search_query:    for recall queries (recall_similar_turns, find_similar_lessons)
-- Previous model: bge-small-en-v1.5 (384-dim). Any 384-dim rows from before
-- the squash would have blocked this ALTER — confirmed zero rows above.
