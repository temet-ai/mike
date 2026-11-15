-- Learning promotion used to render each project's accepted lessons into artifacts/learning-memory/<project-uuid>
-- inside the mike checkout, register that folder as a location of the project and upload it as the knowledge source
-- accepted-learning-memory, so every lesson was stored a second time as a knowledge document (temet-ai/mike#289).
-- Lessons stay lesson rows; this removes the copy, its embeddings and the locations that claimed a project lived
-- inside the mike checkout. A location still referenced by a session or a lesson file receipt is kept.
CREATE TEMP TABLE learning_memory_revisions ON COMMIT DROP AS
SELECT r.id, r.document_uuid
  FROM memory.knowledge_sources s
  JOIN memory.knowledge_documents d ON d.source_uuid = s.source_uuid
  JOIN memory.knowledge_revisions r ON r.document_uuid = d.document_uuid
 WHERE s.name = 'accepted-learning-memory';

DELETE FROM memory.knowledge_embedding_state e
 USING learning_memory_revisions r
 WHERE e.source_kind = 'artifact' AND e.source_id = r.id;

DELETE FROM memory.knowledge_items i
 USING learning_memory_revisions r
 WHERE i.kind = 'artifact' AND i.source_id = r.id;

SET CONSTRAINTS ALL DEFERRED;

DELETE FROM memory.knowledge_revisions r
 USING learning_memory_revisions g
 WHERE r.id = g.id;

DELETE FROM memory.knowledge_documents d
 USING memory.knowledge_sources s
 WHERE d.source_uuid = s.source_uuid AND s.name = 'accepted-learning-memory';

DELETE FROM memory.knowledge_sources s WHERE s.name = 'accepted-learning-memory';

DELETE FROM projects.locations l
 WHERE l.root_path LIKE '%/artifacts/learning-memory/%'
   AND NOT EXISTS (SELECT 1 FROM memory.knowledge_sources s WHERE s.location_uuid = l.location_uuid)
   AND NOT EXISTS (SELECT 1 FROM sessions.sessions x WHERE x.location_uuid = l.location_uuid)
   AND NOT EXISTS (SELECT 1 FROM memory.lesson_file_receipts f WHERE f.location_uuid = l.location_uuid);
