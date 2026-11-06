INSERT INTO memory.knowledge_documents(source_uuid, relative_path)
SELECT '664e249b-93b2-4896-be25-0b43aab4895b'::uuid, x.relative_path FROM (SELECT relative_path FROM memory.knowledge_documents WHERE source_uuid='664e249b-93b2-4896-be25-0b43aab4895b' AND deleted_at IS NULL) x
ON CONFLICT(source_uuid, relative_path) DO UPDATE SET deleted_at = NULL
