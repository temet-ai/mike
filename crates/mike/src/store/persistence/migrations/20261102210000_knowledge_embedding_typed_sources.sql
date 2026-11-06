-- Knowledge embedding state is keyed by (source_kind, source_id) and detects change from a
-- version the source keeps, not from hashing every source on every read:
--   artifact  the revision id itself (revisions never change; a new revision is a new id)
--   note      workspace.session_notes.updated_at (the only note update path sets it)
--   thread    the thread's newest message id (not every append path moves last_message_at)
--   lesson    lessons.text_md5, computed once at write (lesson text is edited in place and
--             lessons have no updated_at)
-- Existing embeddings whose stored content hash still matches their source keep their
-- vectors; the rest are left without a version and are embedded again.

SET LOCAL statement_timeout = 0;
SET LOCAL lock_timeout = '60s';

ALTER TABLE memory.lessons
    ADD COLUMN IF NOT EXISTS text_md5 text GENERATED ALWAYS AS (md5(text)) STORED NOT NULL;

CREATE TEMP TABLE knowledge_fresh ON COMMIT DROP AS
SELECT s.reference
  FROM memory.knowledge_embedding_sources_v s
  JOIN memory.knowledge_embedding_state e
    ON e.reference = s.reference AND e.content_hash = s.content_hash;

ALTER TABLE memory.knowledge_embedding_state
    ADD COLUMN id bigint GENERATED ALWAYS AS IDENTITY,
    ADD COLUMN source_kind text,
    ADD COLUMN source_id bigint,
    ADD COLUMN source_updated_at timestamptz,
    ADD COLUMN source_seq bigint,
    ADD COLUMN source_hash text;

UPDATE memory.knowledge_embedding_state e
   SET source_kind = 'artifact', source_id = r.id
  FROM memory.knowledge_revisions r
 WHERE e.reference = 'artifact:' || r.id;

UPDATE memory.knowledge_embedding_state e
   SET source_kind = 'lesson', source_id = l.id,
       source_hash = CASE WHEN f.reference IS NOT NULL THEN l.text_md5 END
  FROM memory.lessons l
  LEFT JOIN knowledge_fresh f ON f.reference = 'lesson:' || l.uuid
 WHERE e.reference = 'lesson:' || l.uuid;

UPDATE memory.knowledge_embedding_state e
   SET source_kind = 'note', source_id = n.id,
       source_updated_at = CASE WHEN f.reference IS NOT NULL THEN n.updated_at END
  FROM workspace.session_notes n
  LEFT JOIN knowledge_fresh f ON f.reference = 'note:' || n.note_uuid
 WHERE e.reference = 'note:' || n.note_uuid;

UPDATE memory.knowledge_embedding_state e
   SET source_kind = 'thread', source_id = t.id,
       source_seq = CASE WHEN f.reference IS NOT NULL THEN last.message_id END
  FROM mailbox.threads t
  CROSS JOIN LATERAL (SELECT max(m.id) AS message_id
                        FROM mailbox.messages m WHERE m.thread_id = t.id) last
  LEFT JOIN knowledge_fresh f ON f.reference = 'thread:' || t.thread_uuid
 WHERE e.reference = 'thread:' || t.thread_uuid;

-- Rows whose source no longer exists cannot be keyed; their chunks go with them.
DELETE FROM memory.knowledge_embedding_state WHERE source_kind IS NULL;

ALTER TABLE memory.knowledge_embedding_chunks ADD COLUMN state_id bigint;

UPDATE memory.knowledge_embedding_chunks c
   SET state_id = e.id
  FROM memory.knowledge_embedding_state e
 WHERE e.reference = c.reference;

ALTER TABLE memory.knowledge_embedding_chunks
    DROP CONSTRAINT knowledge_embedding_chunks_reference_fkey,
    DROP CONSTRAINT knowledge_embedding_chunks_pkey,
    DROP COLUMN reference;

DROP VIEW memory.knowledge_embedding_sources_v;

ALTER TABLE memory.knowledge_embedding_state
    DROP CONSTRAINT knowledge_embedding_state_pkey,
    DROP COLUMN reference,
    DROP COLUMN content_hash,
    ADD PRIMARY KEY (id),
    ALTER COLUMN source_kind SET NOT NULL,
    ALTER COLUMN source_id SET NOT NULL,
    ADD CONSTRAINT knowledge_embedding_state_source_kind_check
        CHECK (source_kind IN ('artifact', 'lesson', 'note', 'thread')),
    ADD CONSTRAINT knowledge_embedding_state_version_shape_check
        CHECK ((source_kind = 'note' OR source_updated_at IS NULL)
           AND (source_kind = 'thread' OR source_seq IS NULL)
           AND (source_kind = 'lesson' OR source_hash IS NULL));

CREATE UNIQUE INDEX knowledge_embedding_state_source_key
    ON memory.knowledge_embedding_state (source_kind, source_id);

ALTER TABLE memory.knowledge_embedding_chunks
    ALTER COLUMN state_id SET NOT NULL,
    ADD PRIMARY KEY (state_id, ordinal),
    ADD CONSTRAINT knowledge_embedding_chunks_state_id_fkey
        FOREIGN KEY (state_id) REFERENCES memory.knowledge_embedding_state(id) ON DELETE CASCADE;

CREATE VIEW memory.knowledge_embedding_sources_v AS
SELECT 'artifact'::text AS source_kind, d.current_revision AS source_id,
       'artifact:' || d.current_revision AS reference, loc.project_id,
       NULL::timestamptz AS source_updated_at, NULL::bigint AS source_seq, NULL::text AS source_hash
  FROM memory.knowledge_documents d
  JOIN memory.knowledge_sources src ON src.source_uuid = d.source_uuid
  JOIN projects.locations loc ON loc.location_uuid = src.location_uuid
 WHERE d.deleted_at IS NULL
UNION ALL
SELECT 'lesson', l.id, 'lesson:' || l.uuid, l.project_id, NULL, NULL, l.text_md5
  FROM memory.lessons l
 WHERE l.project_id IS NOT NULL
UNION ALL
SELECT 'note', n.id, 'note:' || n.note_uuid, s.project_id, n.updated_at, NULL, NULL
  FROM workspace.session_notes n
  JOIN sessions.sessions s ON s.id = n.session_id
UNION ALL
SELECT 'thread', t.id, 'thread:' || t.thread_uuid, t.project_id, NULL, last.message_id, NULL
  FROM mailbox.threads t
  CROSS JOIN LATERAL (SELECT max(m.id) AS message_id
                        FROM mailbox.messages m WHERE m.thread_id = t.id) last
 WHERE t.project_id IS NOT NULL AND t.last_message_at < now() - interval '10 minutes';

COMMENT ON VIEW memory.knowledge_embedding_sources_v IS
    'Every source project knowledge embeds, with the version its embedding must match: artifacts by revision id, notes by updated_at, threads by newest message id, lessons by text_md5. Bodies are read from the source tables only for the sources being embedded.';
