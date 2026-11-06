INSERT INTO memory.session_embeddings (session_id, embedding, embedding_model, source, chunk_count, source_updated_at)
VALUES ('27a53019-05d3-4797-8776-958f492940d1'::uuid, array_fill(0::real, ARRAY[768])::vector, 'perf-test', 'overview', 0, now())
ON CONFLICT (session_id) DO UPDATE SET embedding_model = EXCLUDED.embedding_model;
