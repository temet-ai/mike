WITH due AS (
    SELECT i.id, i.kind, i.source_id, i.project_id, i.consolidate_at, i.changed_at,
           e.reference, e.source_hash
      FROM memory.knowledge_items i
      JOIN memory.knowledge_embedding_state e ON e.item_id = i.id AND e.reference IS NOT NULL
     WHERE i.consolidate_at IS NOT NULL AND i.consolidate_at <= now() AND NOT i.needs_embedding
     ORDER BY i.consolidate_at, i.id
     LIMIT $1
       FOR UPDATE OF i SKIP LOCKED
), canon AS (
    SELECT c.item_id, c.canonical_item_id
      FROM memory.knowledge_twin_canonical(ARRAY(SELECT d.id FROM due d)) c
)
SELECT d.id AS "item_id!", d.kind AS "kind!", a.source_name AS "source_name?", d.project_id AS "project_id?",
       d.reference AS "reference!",
       CASE d.kind WHEN 'lesson' THEN l.text WHEN 'artifact' THEN r.body END AS "body?",
       COALESCE(d.source_hash, r.content_hash, l.text_md5, d.changed_at::text) AS "source_hash!",
       COALESCE(c.canonical_item_id, d.id) AS "canonical_item_id!",
       d.consolidate_at AS "consolidate_at!",
       COALESCE(l.created_at, r.observed_at, d.changed_at) AS "stated_at!",
       CASE d.kind
            WHEN 'lesson' THEN l.archived_at IS NULL AND l.retired_at IS NULL
            WHEN 'artifact' THEN COALESCE(a.current, false)
            ELSE true END AS "active!",
       (d.kind = 'artifact' AND EXISTS (
            SELECT 1 FROM memory.knowledge_statements x
             WHERE x.document_uuid = r.document_uuid AND x.item_id IS NULL AND x.state <> 'gone')) AS "detached_statements!"
  FROM due d
  LEFT JOIN canon c ON c.item_id = d.id
  LEFT JOIN memory.lessons l ON d.kind = 'lesson' AND l.id = d.source_id
  LEFT JOIN memory.knowledge_revisions r ON d.kind = 'artifact' AND r.id = d.source_id
  LEFT JOIN memory.knowledge_artifact_identity_v a ON d.kind = 'artifact' AND a.revision_id = d.source_id
 ORDER BY d.consolidate_at, d.id
