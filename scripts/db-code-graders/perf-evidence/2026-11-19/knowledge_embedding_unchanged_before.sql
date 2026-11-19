WITH d AS (DELETE FROM memory.knowledge_embedding_chunks WHERE state_id = 179230 RETURNING *)
INSERT INTO memory.knowledge_embedding_chunks SELECT * FROM d
