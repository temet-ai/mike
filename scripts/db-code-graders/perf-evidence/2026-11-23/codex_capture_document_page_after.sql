SELECT c.id, r.ordinal, c.body FROM runtime.capture_document_chunks r
JOIN runtime.capture_chunks c ON c.id = r.chunk_id
WHERE r.document_id = 50 AND r.ordinal >= 400 ORDER BY r.ordinal LIMIT 256;
