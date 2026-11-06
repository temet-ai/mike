WITH semantic_chunks AS (
    SELECT c.reference, 1 - (c.embedding <=> NULL::public.vector) AS score
    FROM memory.knowledge_embedding_chunks c
    JOIN memory.knowledge_embedding_state e USING(reference)
    JOIN memory.knowledge_embedding_sources_v src USING(reference)
    WHERE src.project_id = (SELECT id FROM projects.projects WHERE project_uuid = 'ec5bcb17-7792-47c2-9f7b-c07b1f417f92') AND e.content_hash = src.content_hash
      AND e.model_key = NULL AND e.completed_at IS NOT NULL AND NULL::text IS NOT NULL
    UNION ALL
    SELECT 'turn:' || c.turn_id, 1 - (c.chunk_embedding <=> NULL::public.vector)
    FROM memory.operator_turn_chunks c JOIN memory.operator_turns t ON t.id = c.turn_id
    WHERE t.project_id = (SELECT id FROM projects.projects WHERE project_uuid = 'ec5bcb17-7792-47c2-9f7b-c07b1f417f92') AND c.embedding_model = NULL AND NULL::text IS NOT NULL
      AND (t.opened_by IS NULL OR t.opened_by = 'operator')
), semantic AS (
    SELECT reference, max(score) AS score FROM semantic_chunks GROUP BY reference HAVING max(score) >= 0.3
), q AS (SELECT websearch_to_tsquery('english', 'worktree') AS tsq), superseded AS (
    SELECT DISTINCT rel.to_lesson_id
    FROM memory.lesson_relations rel
    JOIN memory.lesson_relation_kinds rk ON rk.id = rel.kind_id AND rk.code = 'supersedes'
    JOIN memory.lesson_relation_statuses rs ON rs.id = rel.status_id AND rs.code = 'confirmed'
), candidates AS (
    SELECT 'artifact:' || r.id AS reference, 'artifact' AS source_kind, r.title,
           r.status, d.relative_path AS path, r.observed_at AS updated_at,
           ts_rank_cd(r.search_tsv, q.tsq) AS rank, COALESCE(sem.score, 0) AS semantic_rank,
           jsonb_build_object('document_uuid', d.document_uuid, 'revision', r.id,
             'location_uuid', s.location_uuid, 'last_scan_at', s.last_scan_at,
             'deleted', d.deleted_at IS NOT NULL, 'metadata', r.metadata) AS provenance
      FROM memory.knowledge_documents d
      JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
      JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
      JOIN memory.knowledge_revisions r ON r.id = d.current_revision CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'artifact:' || r.id
     WHERE loc.project_id = (SELECT id FROM projects.projects WHERE project_uuid = 'ec5bcb17-7792-47c2-9f7b-c07b1f417f92') AND ('worktree' = '' OR r.search_tsv @@ q.tsq OR sem.score IS NOT NULL)
       AND s.name NOT IN ('claude-memory', 'skills')
       AND (false OR (d.deleted_at IS NULL AND r.status NOT IN ('superseded', 'archived', 'rejected')))
    UNION ALL
    SELECT 'artifact:' || r.id AS reference,
           CASE s.name WHEN 'claude-memory' THEN 'memory' ELSE 'skill' END AS source_kind, r.title,
           r.status, d.relative_path AS path, r.observed_at AS updated_at,
           ts_rank_cd(r.search_tsv, q.tsq) AS rank, COALESCE(sem.score, 0) AS semantic_rank,
           jsonb_build_object('document_uuid', d.document_uuid, 'revision', r.id,
             'location_uuid', s.location_uuid, 'last_scan_at', s.last_scan_at,
             'deleted', d.deleted_at IS NOT NULL, 'metadata', r.metadata) AS provenance
      FROM memory.knowledge_documents d
      JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
      JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
      JOIN memory.knowledge_revisions r ON r.id = d.current_revision CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'artifact:' || r.id
     WHERE loc.project_id = (SELECT id FROM projects.projects WHERE project_uuid = 'ec5bcb17-7792-47c2-9f7b-c07b1f417f92') AND ('worktree' = '' OR r.search_tsv @@ q.tsq OR sem.score IS NOT NULL)
       AND s.name IN ('claude-memory', 'skills')
       AND (false OR (d.deleted_at IS NULL AND r.status NOT IN ('superseded', 'archived', 'rejected')))
    UNION ALL
    SELECT 'lesson:' || l.uuid, 'lesson', k.code, st.code, NULL, l.created_at,
           ts_rank_cd(to_tsvector('english', l.text), q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('lesson_id', l.id, 'topic', l.topic, 'source_turn_id', l.source_turn_id)
      FROM memory.lessons l JOIN memory.lesson_kinds k ON k.id = l.kind_id
      JOIN memory.lesson_stages st ON st.id = l.stage_id
      LEFT JOIN superseded sup ON sup.to_lesson_id = l.id CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'lesson:' || l.uuid
     WHERE l.project_id = (SELECT id FROM projects.projects WHERE project_uuid = 'ec5bcb17-7792-47c2-9f7b-c07b1f417f92') AND ('worktree' = '' OR to_tsvector('english', l.text) @@ q.tsq OR sem.score IS NOT NULL)
       AND (false OR (l.archived_at IS NULL AND (l.expires_at IS NULL OR l.expires_at > now())
         AND sup.to_lesson_id IS NULL))
    UNION ALL
    SELECT 'note:' || n.note_uuid, 'note', n.title, COALESCE(n.status, 'unknown'), NULL,
           n.updated_at, ts_rank_cd(n.search_tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('session_uuid', ss.session_uuid, 'kind', n.kind, 'tags', n.tags)
      FROM workspace.session_notes n JOIN sessions.sessions ss ON ss.id = n.session_id CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'note:' || n.note_uuid
     WHERE ss.project_id = (SELECT id FROM projects.projects WHERE project_uuid = 'ec5bcb17-7792-47c2-9f7b-c07b1f417f92') AND ('worktree' = '' OR n.search_tsv @@ q.tsq OR sem.score IS NOT NULL)
       AND (false OR COALESCE(n.status, '') NOT IN ('superseded', 'archived', 'rejected'))
    UNION ALL
    SELECT 'turn:' || t.id, 'turn', COALESCE(t.user_prompt, ''), 'historical', NULL,
           t.created_at, ts_rank_cd(t.search_tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('session_uuid', ss.session_uuid)
      FROM memory.operator_turns t JOIN sessions.sessions ss ON ss.id = t.session_id CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'turn:' || t.id
     WHERE t.project_id = (SELECT id FROM projects.projects WHERE project_uuid = 'ec5bcb17-7792-47c2-9f7b-c07b1f417f92') AND ('worktree' = '' OR t.search_tsv @@ q.tsq OR sem.score IS NOT NULL)
       AND (t.opened_by IS NULL OR t.opened_by = 'operator')
    UNION ALL
    SELECT 'thread:' || th.thread_uuid, 'thread', th.subject,
           CASE WHEN th.resolved_at IS NULL THEN 'open' ELSE 'resolved' END, NULL,
           th.last_message_at, COALESCE(hit.rank, 0), COALESCE(sem.score, 0),
           jsonb_build_object('thread_uuid', th.thread_uuid, 'fleet_uuid', th.fleet_uuid)
      FROM mailbox.threads th CROSS JOIN q
      LEFT JOIN (SELECT m.thread_id, max(ts_rank_cd(m.search_tsv, q.tsq)) AS rank
                   FROM mailbox.messages m CROSS JOIN q
                  WHERE 'worktree' <> '' AND m.thread_id IS NOT NULL AND m.search_tsv @@ q.tsq
                  GROUP BY m.thread_id) hit ON hit.thread_id = th.id
      LEFT JOIN semantic sem ON sem.reference = 'thread:' || th.thread_uuid
     WHERE th.project_id = (SELECT id FROM projects.projects WHERE project_uuid = 'ec5bcb17-7792-47c2-9f7b-c07b1f417f92') AND ('worktree' = '' OR hit.thread_id IS NOT NULL OR sem.score IS NOT NULL)
), ranked AS (
    SELECT *, CASE WHEN rank > 0 THEN 1.0 / (60 + row_number() OVER (ORDER BY rank DESC, reference)) ELSE 0 END
      + CASE WHEN semantic_rank > 0 THEN 1.0 / (60 + row_number() OVER (ORDER BY semantic_rank DESC, reference)) ELSE 0 END AS fused_rank
    FROM candidates WHERE (false OR source_kind = 'artifact')
)
SELECT jsonb_build_object('reference' , reference, 'source_kind', source_kind,
    'title', title, 'status', status, 'path', path, 'updated_at', updated_at,
    'rank', fused_rank, 'lexical_rank', rank, 'semantic_rank', semantic_rank, 'provenance', provenance)
FROM ranked
ORDER BY fused_rank DESC, updated_at DESC, reference LIMIT 21 OFFSET 0
