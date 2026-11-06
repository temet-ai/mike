WITH q AS (SELECT websearch_to_tsquery('english', 'memory') AS tsq), superseded AS (
    SELECT DISTINCT rel.to_lesson_id
    FROM memory.lesson_relations rel
    JOIN memory.lesson_relation_kinds rk ON rk.id = rel.kind_id AND rk.code = 'supersedes'
    JOIN memory.lesson_relation_statuses rs ON rs.id = rel.status_id AND rs.code = 'confirmed'
), candidates AS (
    SELECT 'artifact:' || r.id AS reference, 'artifact' AS source_kind, r.title,
           r.status, d.relative_path AS path, r.observed_at AS updated_at,
           ts_rank_cd(r.search_tsv, q.tsq) AS rank,
           jsonb_build_object('document_uuid', d.document_uuid, 'revision', r.id,
             'location_uuid', s.location_uuid, 'last_scan_at', s.last_scan_at,
             'deleted', d.deleted_at IS NOT NULL, 'metadata', r.metadata) AS provenance
      FROM memory.knowledge_documents d
      JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
      JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
      JOIN memory.knowledge_revisions r ON r.id = d.current_revision CROSS JOIN q
     WHERE loc.project_id = 993478 AND ('memory' = '' OR r.search_tsv @@ q.tsq)
       AND (false OR (d.deleted_at IS NULL AND r.status NOT IN ('superseded', 'archived', 'rejected')))
    UNION ALL
    SELECT 'lesson:' || l.uuid, 'lesson', k.code, st.code, NULL, l.created_at,
           ts_rank_cd(to_tsvector('english', l.text), q.tsq),
           jsonb_build_object('lesson_id', l.id, 'topic', l.topic, 'source_turn_id', l.source_turn_id)
      FROM memory.lessons l JOIN memory.lesson_kinds k ON k.id = l.kind_id
      JOIN memory.lesson_stages st ON st.id = l.stage_id
      LEFT JOIN superseded sup ON sup.to_lesson_id = l.id CROSS JOIN q
     WHERE l.project_id = 993478 AND ('memory' = '' OR to_tsvector('english', l.text) @@ q.tsq)
       AND (false OR (l.archived_at IS NULL AND (l.expires_at IS NULL OR l.expires_at > now())
         AND sup.to_lesson_id IS NULL))
    UNION ALL
    SELECT 'note:' || n.note_uuid, 'note', n.title, COALESCE(n.status, 'unknown'), NULL,
           n.updated_at, ts_rank_cd(to_tsvector('english', n.title || ' ' || n.body), q.tsq),
           jsonb_build_object('session_uuid', ss.session_uuid, 'kind', n.kind, 'tags', n.tags)
      FROM workspace.session_notes n JOIN sessions.sessions ss ON ss.id = n.session_id CROSS JOIN q
     WHERE ss.project_id = 993478 AND ('memory' = '' OR to_tsvector('english', n.title || ' ' || n.body) @@ q.tsq)
       AND (false OR COALESCE(n.status, '') NOT IN ('superseded', 'archived', 'rejected'))
    UNION ALL
    SELECT 'turn:' || t.id, 'turn', COALESCE(t.user_prompt, ''), 'historical', NULL,
           t.created_at, ts_rank_cd(t.search_tsv, q.tsq),
           jsonb_build_object('session_uuid', ss.session_uuid)
      FROM memory.operator_turns t JOIN sessions.sessions ss ON ss.id = t.session_id CROSS JOIN q
     WHERE t.project_id = 993478 AND ('memory' = '' OR t.search_tsv @@ q.tsq)
)
SELECT jsonb_build_object('reference', reference, 'source_kind', source_kind,
    'title', title, 'status', status, 'path', path, 'updated_at', updated_at,
    'rank', rank, 'provenance', provenance)
FROM candidates WHERE (NULL::text::text IS NULL OR source_kind = NULL::text)
ORDER BY rank DESC, updated_at DESC, reference LIMIT 21 OFFSET 0
