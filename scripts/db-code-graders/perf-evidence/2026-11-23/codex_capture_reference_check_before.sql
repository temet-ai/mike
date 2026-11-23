SELECT NOT EXISTS (
 SELECT 1 FROM UNNEST(ARRAY(SELECT generate_series(400,655))::bigint[]) AS wanted(id)
 WHERE NOT EXISTS (SELECT 1 FROM runtime.capture_document_chunks r WHERE r.document_id = 50 AND r.chunk_id = wanted.id)
);
