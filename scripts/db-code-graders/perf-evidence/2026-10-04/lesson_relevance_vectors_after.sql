SELECT id, embedding::text FROM memory.lessons WHERE embedding_model = 'nomic-embed-text-v1.5' AND embedding IS NOT NULL AND archived_at IS NULL;
