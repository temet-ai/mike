-- Phase 13 memory recall C0.1–C0.4: topic column + harvester source +
-- worker-predicate index on operator_turns.
--
-- C0.1: topic column (nullable, no default — existing rows get NULL,
-- which is correct: only harvester-written lessons carry a topic).
ALTER TABLE memory.lessons ADD COLUMN IF NOT EXISTS topic text;

COMMENT ON COLUMN memory.lessons.topic IS
    'Optional topic key for verb-grammar recall (mem:recall/mem:update tags). '
    'NULL for lessons written before Phase 13 or via the MCP record_lesson tool. '
    'B-tree indexed for exact-match recall (idx_lessons_topic). '
    'Never truncated — inspection-fidelity applies.';

-- C0.2: B-tree partial index for topic-exact recall.
-- Covers Component A query:
--   WHERE project_id = $1 AND topic = $2 AND archived_at IS NULL
--   AND (expires_at IS NULL OR expires_at > NOW())
-- The partial predicate excludes archived rows (archived_at IS NOT NULL
-- corresponds to stage_id = 4, enforced by lessons_stage_archive_consistent).
CREATE INDEX IF NOT EXISTS idx_lessons_topic ON memory.lessons
    USING btree (project_id, topic)
    WHERE topic IS NOT NULL AND archived_at IS NULL;

-- C0.3: harvester source row (id=4).
-- Existing rows: 1='agent', 2='operator', 3='copilot'.
-- id=4 is explicit (OVERRIDING SYSTEM VALUE) to avoid identity-sequence
-- collisions with future additions; the sequence is advanced past 4 by the
-- subsequent setval so plain INSERTs keep allocating monotonically from 5+.
INSERT INTO memory.lesson_sources (id, code) OVERRIDING SYSTEM VALUE VALUES
    (4, 'harvester')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.lesson_sources_id_seq', 4);

-- C0.4: partial index for the Phase 13/15 embedding worker.
-- Worker query: WHERE closed_at IS NOT NULL AND embedded_at IS NULL
-- Confirmed absent from baseline.sql lines 911–915.
CREATE INDEX IF NOT EXISTS idx_operator_turns_pending_embed ON memory.operator_turns
    USING btree (project_id, closed_at)
    WHERE closed_at IS NOT NULL AND embedded_at IS NULL;
