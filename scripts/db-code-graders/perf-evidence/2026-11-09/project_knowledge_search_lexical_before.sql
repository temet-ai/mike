WITH knowledge_scope AS MATERIALIZED (
    SELECT e.id AS state_id, src.reference
    FROM memory.knowledge_embedding_state e
    JOIN memory.knowledge_embedding_sources_v src
      ON src.source_kind = e.source_kind AND src.source_id = e.source_id
    WHERE src.project_id = 993478
      AND e.source_updated_at IS NOT DISTINCT FROM src.source_updated_at
      AND e.source_seq IS NOT DISTINCT FROM src.source_seq
      AND e.source_hash IS NOT DISTINCT FROM src.source_hash
      AND e.model_key = '44504dc8418beb54a3300bc02c99c0ac2c667ac837d1c8e6234bf25241084f28' AND e.completed_at IS NOT NULL AND NULL::text IS NOT NULL
), nearest_first AS (
    SELECT (SELECT count(*) FROM knowledge_scope) * 5
           >= (SELECT reltuples FROM pg_class WHERE oid = 'memory.knowledge_embedding_state'::regclass) AS enabled
), nearest_knowledge_chunks AS MATERIALIZED (
    SELECT c.state_id, c.embedding <=> NULL::text::public.vector AS distance
    FROM memory.knowledge_embedding_chunks c
    WHERE (SELECT enabled FROM nearest_first)
    ORDER BY c.embedding <=> NULL::text::public.vector
    LIMIT 1000
), semantic_chunks AS (
    (SELECT s.reference, 1 - (c.embedding <=> NULL::text::public.vector) AS score
     FROM knowledge_scope s
     JOIN memory.knowledge_embedding_chunks c ON c.state_id = s.state_id
     WHERE NOT (SELECT enabled FROM nearest_first)
     ORDER BY c.embedding <=> NULL::text::public.vector
     LIMIT 200)
    UNION ALL
    (SELECT s.reference, 1 - n.distance
     FROM nearest_knowledge_chunks n JOIN knowledge_scope s ON s.state_id = n.state_id
     ORDER BY n.distance
     LIMIT 200)
    UNION ALL
    (SELECT 'turn:' || c.turn_id, 1 - (c.chunk_embedding <=> NULL::text::public.vector)
     FROM memory.operator_turn_chunks c JOIN memory.operator_turns t ON t.id = c.turn_id
     WHERE t.project_id = 993478 AND c.embedding_model = 'nomic-embed-text-v1.5' AND NULL::text IS NOT NULL
       AND (t.opened_by IS NULL OR t.opened_by = 'operator')
     ORDER BY c.chunk_embedding <=> NULL::text::public.vector
     LIMIT 200)
), semantic AS (
    SELECT reference, score
    FROM (SELECT reference, max(score) AS score, max(max(score)) OVER () AS best
          FROM semantic_chunks
          WHERE NULL::text::text IS NULL OR split_part(reference, ':', 1)
                = CASE WHEN NULL::text::text IN ('memory', 'skill') THEN 'artifact' ELSE NULL::text::text END
          GROUP BY reference) per_reference
    WHERE score >= GREATEST(0.6, best - 0.08)
), q AS (SELECT websearch_to_tsquery('english', 'rust target filling the disk') AS tsq), superseded AS (
    SELECT DISTINCT rel.to_lesson_id
    FROM memory.lesson_relations rel
    JOIN memory.lesson_relation_kinds rk ON rk.id = rel.kind_id AND rk.code = 'supersedes'
    JOIN memory.lesson_relation_statuses rs ON rs.id = rel.status_id AND rs.code = 'confirmed'
), source_freshness AS (
    SELECT s.source_uuid, jsonb_build_object(
               'state', CASE WHEN jsonb_array_length(fr.reasons) = 0 THEN 'fresh' ELSE 'stale' END,
               'reasons', fr.reasons,
               'last_scan_at', s.last_scan_at,
               'last_verified_at', GREATEST(s.last_scan_at, s.last_heartbeat_at)) AS freshness
      FROM memory.knowledge_sources s
      JOIN projects.locations sl ON sl.location_uuid = s.location_uuid
     CROSS JOIN LATERAL (
        SELECT COALESCE(jsonb_agg(f.reason), '[]'::jsonb) AS reasons
          FROM (VALUES
            (CASE WHEN s.last_failure IS NOT NULL AND s.last_failure_generation = s.generation
                  THEN 'the source''s latest scan failed: ' || s.last_failure END),
            (CASE WHEN GREATEST(s.last_scan_at, s.last_heartbeat_at) IS NULL
                  THEN 'the source has never been scanned'
                  WHEN GREATEST(s.last_scan_at, s.last_heartbeat_at) < now() - interval '1 hour'
                  THEN 'no scanner has checked the source since '
                       || to_char(GREATEST(s.last_scan_at, s.last_heartbeat_at) AT TIME ZONE 'UTC',
                                  'YYYY-MM-DD"T"HH24:MI:SS"Z"') END)
          ) AS f(reason)
         WHERE f.reason IS NOT NULL
     ) fr
     WHERE sl.project_id = 993478
), candidates AS (
    SELECT 'artifact:' || r.id AS reference, 'artifact' AS source_kind, r.title,
           r.status, d.relative_path AS path, r.observed_at AS updated_at,
           ts_rank_cd(r.search_tsv, q.tsq) AS rank, COALESCE(sem.score, 0) AS semantic_rank,
           jsonb_build_object('document_uuid', d.document_uuid, 'revision', r.id,
             'location_uuid', s.location_uuid, 'last_scan_at', s.last_scan_at,
             'deleted', d.deleted_at IS NOT NULL, 'metadata', r.metadata,
             'freshness', sf.freshness) AS provenance
      FROM memory.knowledge_documents d
      JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
      JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
      JOIN source_freshness sf ON sf.source_uuid = s.source_uuid
      JOIN memory.knowledge_revisions r ON r.id = d.current_revision CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'artifact:' || r.id
     WHERE loc.project_id = 993478 AND ('rust target filling the disk' = '' OR r.search_tsv @@ q.tsq OR sem.score IS NOT NULL)
       AND s.name NOT IN ('claude-memory', 'skills')
       AND (false OR (d.deleted_at IS NULL AND r.status NOT IN ('superseded', 'archived', 'rejected')))
    UNION ALL
    SELECT 'artifact:' || r.id AS reference,
           CASE s.name WHEN 'claude-memory' THEN 'memory' ELSE 'skill' END AS source_kind, r.title,
           r.status, d.relative_path AS path, r.observed_at AS updated_at,
           ts_rank_cd(r.search_tsv, q.tsq) AS rank, COALESCE(sem.score, 0) AS semantic_rank,
           jsonb_build_object('document_uuid', d.document_uuid, 'revision', r.id,
             'location_uuid', s.location_uuid, 'last_scan_at', s.last_scan_at,
             'deleted', d.deleted_at IS NOT NULL, 'metadata', r.metadata,
             'freshness', sf.freshness) AS provenance
      FROM memory.knowledge_documents d
      JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
      JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
      JOIN source_freshness sf ON sf.source_uuid = s.source_uuid
      JOIN memory.knowledge_revisions r ON r.id = d.current_revision CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'artifact:' || r.id
     WHERE loc.project_id = 993478 AND ('rust target filling the disk' = '' OR r.search_tsv @@ q.tsq OR sem.score IS NOT NULL)
       AND s.name IN ('claude-memory', 'skills')
       AND (false OR (d.deleted_at IS NULL AND r.status NOT IN ('superseded', 'archived', 'rejected')))
    UNION ALL
    SELECT 'lesson:' || l.uuid, 'lesson', k.code, st.code, NULL, l.created_at,
           ts_rank_cd(to_tsvector('english', l.text), q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('lesson_id', l.id, 'topic', l.topic, 'source_turn_id', l.source_turn_id)
      FROM memory.lessons l JOIN memory.lesson_kinds k ON k.id = l.kind_id
      JOIN memory.lesson_stages st ON st.id = l.stage_id
      LEFT JOIN superseded sup ON sup.to_lesson_id = l.id CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'lesson:' || l.uuid
     WHERE l.project_id = 993478 AND ('rust target filling the disk' = '' OR to_tsvector('english', l.text) @@ q.tsq OR sem.score IS NOT NULL)
       AND (false OR (l.archived_at IS NULL AND (l.expires_at IS NULL OR l.expires_at > now())
         AND sup.to_lesson_id IS NULL))
    UNION ALL
    SELECT 'note:' || n.note_uuid, 'note', n.title, COALESCE(n.status, 'unknown'), NULL,
           n.updated_at, ts_rank_cd(n.search_tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('session_uuid', ss.session_uuid, 'kind', n.kind, 'tags', n.tags)
      FROM workspace.session_notes n JOIN sessions.sessions ss ON ss.id = n.session_id CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'note:' || n.note_uuid
     WHERE ss.project_id = 993478 AND ('rust target filling the disk' = '' OR n.search_tsv @@ q.tsq OR sem.score IS NOT NULL)
       AND (false OR COALESCE(n.status, '') NOT IN ('superseded', 'archived', 'rejected'))
    UNION ALL
    SELECT 'turn:' || t.id, 'turn', COALESCE(t.user_prompt, ''), 'historical', NULL,
           t.created_at, ts_rank_cd(t.search_tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('session_uuid', ss.session_uuid)
      FROM memory.operator_turns t JOIN sessions.sessions ss ON ss.id = t.session_id CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'turn:' || t.id
     WHERE t.project_id = 993478 AND ('rust target filling the disk' = '' OR t.search_tsv @@ q.tsq OR sem.score IS NOT NULL)
       AND (t.opened_by IS NULL OR t.opened_by = 'operator')
    UNION ALL
    SELECT 'thread:' || th.thread_uuid, 'thread', th.subject,
           CASE WHEN th.resolved_at IS NULL THEN 'open' ELSE 'resolved' END, NULL,
           th.last_message_at, COALESCE(hit.rank, 0), COALESCE(sem.score, 0),
           jsonb_build_object('thread_uuid', th.thread_uuid, 'fleet_uuid', th.fleet_uuid)
      FROM mailbox.threads th CROSS JOIN q
      LEFT JOIN (SELECT m.thread_id, max(ts_rank_cd(m.search_tsv, q.tsq)) AS rank
                   FROM mailbox.messages m CROSS JOIN q
                  WHERE 'rust target filling the disk' <> '' AND m.thread_id IS NOT NULL AND m.search_tsv @@ q.tsq
                  GROUP BY m.thread_id) hit ON hit.thread_id = th.id
      LEFT JOIN semantic sem ON sem.reference = 'thread:' || th.thread_uuid
     WHERE th.project_id = 993478 AND ('rust target filling the disk' = '' OR hit.thread_id IS NOT NULL OR sem.score IS NOT NULL)
    UNION ALL
    SELECT 'work_thread:' || wt.slug, 'work_thread', COALESCE(wt.title, wt.slug),
           COALESCE(wt.status, 'unknown'), NULL, wt.updated_at,
           ts_rank_cd(doc.tsv, q.tsq), 0::double precision,
           jsonb_build_object('thread_id', wt.id, 'slug', wt.slug, 'goal', wt.goal)
      FROM work.threads wt LEFT JOIN work.thread_state wts ON wts.thread_id = wt.id CROSS JOIN q
     CROSS JOIN LATERAL (SELECT to_tsvector('english', concat_ws(' ', wt.title, wt.slug, wt.goal,
              wts.tldr, wts.where_we_are, wts.next_up, wts.blockers, wts.open_questions,
              wts.decisions)) AS tsv) doc
     WHERE wt.project_id = 993478 AND ('rust target filling the disk' = '' OR doc.tsv @@ q.tsq)
), ranked AS (
    SELECT *, CASE WHEN rank > 0 THEN 1.0 / (60 + row_number() OVER (ORDER BY rank DESC, reference)) ELSE 0 END
      + CASE WHEN semantic_rank > 0 THEN 1.0 / (60 + row_number() OVER (ORDER BY semantic_rank DESC, reference)) ELSE 0 END AS fused_rank
    FROM candidates WHERE (NULL::text::text IS NULL OR source_kind = NULL::text)
)
SELECT jsonb_build_object('reference' , reference, 'source_kind', source_kind,
    'title', title, 'status', status, 'path', path, 'updated_at', updated_at,
    'rank', fused_rank, 'lexical_rank', rank, 'semantic_rank', semantic_rank, 'provenance', provenance) AS item
FROM ranked
ORDER BY fused_rank DESC, updated_at DESC, reference LIMIT 21 OFFSET 0;
