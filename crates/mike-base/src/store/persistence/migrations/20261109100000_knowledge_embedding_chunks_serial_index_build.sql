-- A parallel HNSW build takes shared memory as large as maintenance_work_mem (1GB here) and fails when the container's shared memory is smaller (512MB in scripts/capture-db/docker-compose.yml), so the next migration builds the index with a single process.
ALTER TABLE memory.knowledge_embedding_chunks SET (parallel_workers = 0);
