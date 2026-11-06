WITH docs AS (
    SELECT 'artifact:' || r.id AS reference, s.name AS source_name, d.relative_path,
           r.title, r.body, r.content_hash, r.metadata, r.search_tsv
    FROM projects.locations loc
    JOIN memory.knowledge_sources s ON s.location_uuid = loc.location_uuid
         AND s.name IN ('claude-memory', 'skills')
    JOIN memory.knowledge_documents d ON d.source_uuid = s.source_uuid AND d.deleted_at IS NULL
    JOIN memory.knowledge_revisions r ON r.id = d.current_revision
    WHERE loc.project_id = (SELECT id FROM projects.projects WHERE project_uuid = 'ec5bcb17-7792-47c2-9f7b-c07b1f417f92') AND r.status NOT IN ('superseded', 'archived', 'rejected')
), q AS (
    SELECT to_tsquery('english', replace(plainto_tsquery('english', 'you claimed the tests pass without running the test suite')::text, ' & ', ' | ')) AS tsq
), semantic AS (
    SELECT c.reference, max(1 - (c.embedding <=> NULL::text::public.vector)) AS score
    FROM docs
    JOIN memory.knowledge_embedding_state e ON e.reference = docs.reference
         AND e.content_hash = docs.content_hash AND e.model_key = NULL
         AND e.completed_at IS NOT NULL
    JOIN memory.knowledge_embedding_chunks c ON c.reference = docs.reference
    WHERE NULL::text IS NOT NULL
    GROUP BY c.reference
), scored AS (
    SELECT DISTINCT ON (docs.source_name, docs.content_hash)
           docs.reference, docs.source_name, docs.relative_path, docs.title, docs.body,
           docs.metadata, sem.score AS similarity,
           CASE WHEN docs.search_tsv @@ q.tsq THEN ts_rank_cd(docs.search_tsv, q.tsq) ELSE 0 END AS lexical
    FROM docs CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = docs.reference
    ORDER BY docs.source_name, docs.content_hash, docs.reference
)
SELECT reference AS "reference!", source_name AS "source_name!", relative_path AS "relative_path!",
       title AS "title!", body AS "body!", metadata AS "metadata!", similarity
FROM scored
WHERE COALESCE(similarity, 0) >= 0.3 OR (similarity IS NULL AND lexical > 0)
ORDER BY COALESCE(similarity, 0) DESC, lexical DESC, reference
LIMIT 3
