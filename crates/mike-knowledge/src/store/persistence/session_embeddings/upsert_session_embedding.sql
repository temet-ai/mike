INSERT INTO memory.session_embeddings
    (session_id, embedding, embedding_model, source, chunk_count,
     source_updated_at, embedded_at)
VALUES ($1, $2::text::vector, $3, $4, $5, $6, now())
ON CONFLICT (session_id) DO UPDATE
   SET embedding          = EXCLUDED.embedding,
       embedding_model    = EXCLUDED.embedding_model,
       source             = EXCLUDED.source,
       chunk_count        = EXCLUDED.chunk_count,
       source_updated_at  = EXCLUDED.source_updated_at,
       embedded_at        = EXCLUDED.embedded_at
