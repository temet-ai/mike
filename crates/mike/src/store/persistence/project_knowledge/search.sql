WITH own_turn AS (
    SELECT max(t.id) AS id
    FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
    WHERE s.session_uuid = $12::uuid AND (t.opened_by IS NULL OR t.opened_by = 'operator')
), own_refs AS (
    SELECT 'turn:' || o.id AS reference FROM own_turn o WHERE o.id IS NOT NULL
    UNION ALL
    SELECT vk.kind || ':' || v.id
    FROM own_turn o
    JOIN memory.live_turn_verdicts_v v ON v.operator_turn_id = o.id
    CROSS JOIN (VALUES ('unmet_ask'), ('correction')) vk(kind)
), knowledge_scope AS MATERIALIZED (
    SELECT e.id AS state_id, src.reference
    FROM memory.knowledge_embedding_state e
    JOIN memory.knowledge_sources_with_sessions_v src
      ON src.source_kind = e.source_kind AND src.source_id = e.source_id
    WHERE src.project_id = $1
      AND e.source_updated_at IS NOT DISTINCT FROM src.source_updated_at
      AND e.source_seq IS NOT DISTINCT FROM src.source_seq
      AND e.source_hash IS NOT DISTINCT FROM src.source_hash
      AND e.model_key = $8 AND e.completed_at IS NOT NULL AND $7::text IS NOT NULL
), nearest_first AS (
    SELECT ($11::bool[])[2] AS enabled
), nearest_knowledge_chunks AS MATERIALIZED (
    SELECT c.state_id, c.embedding <=> $7::text::public.vector AS distance
    FROM memory.knowledge_embedding_chunks c
    WHERE (SELECT enabled FROM nearest_first) AND $7::text IS NOT NULL
    ORDER BY c.embedding <=> $7::text::public.vector
    LIMIT 1000
), semantic_chunks AS (
    (SELECT s.reference, 1 - (c.embedding <=> $7::text::public.vector) AS score
     FROM knowledge_scope s
     JOIN memory.knowledge_embedding_chunks c ON c.state_id = s.state_id
     WHERE NOT (SELECT enabled FROM nearest_first)
     ORDER BY c.embedding <=> $7::text::public.vector
     LIMIT 200)
    UNION ALL
    (SELECT p.reference, p.score
     FROM (
         SELECT s.reference, 1 - n.distance AS score, n.distance
         FROM nearest_knowledge_chunks n
         JOIN memory.knowledge_embedding_state e ON e.id = n.state_id
         CROSS JOIN LATERAL (
             SELECT src.reference
             FROM memory.knowledge_sources_with_sessions_v src
             WHERE src.source_kind = e.source_kind AND src.source_id = e.source_id
               AND src.project_id = $1
               AND e.source_updated_at IS NOT DISTINCT FROM src.source_updated_at
               AND e.source_seq IS NOT DISTINCT FROM src.source_seq
               AND e.source_hash IS NOT DISTINCT FROM src.source_hash
             OFFSET 0) s
         WHERE e.model_key = $8 AND e.completed_at IS NOT NULL AND $7::text IS NOT NULL
           AND e.source_kind <> 'artifact'
         UNION ALL
         SELECT 'artifact:' || r.id, 1 - n.distance, n.distance
         FROM nearest_knowledge_chunks n
         JOIN memory.knowledge_embedding_state e ON e.id = n.state_id AND e.source_kind = 'artifact'
         JOIN memory.knowledge_revisions r ON r.id = e.source_id
         JOIN memory.knowledge_documents d
           ON d.document_uuid = r.document_uuid AND d.current_revision = r.id AND d.deleted_at IS NULL
         JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
         JOIN projects.locations loc ON loc.location_uuid = s.location_uuid AND loc.project_id = $1
         WHERE e.model_key = $8 AND e.completed_at IS NOT NULL AND $7::text IS NOT NULL
           AND e.source_updated_at IS NULL AND e.source_seq IS NULL AND e.source_hash IS NULL
     ) p
     ORDER BY p.distance
     LIMIT 200)
    UNION ALL
    (SELECT 'turn:' || c.turn_id, 1 - (c.chunk_embedding <=> $7::text::public.vector)
     FROM memory.operator_turn_chunks c JOIN memory.operator_turns t ON t.id = c.turn_id
     WHERE t.project_id = $1 AND c.embedding_model = $9 AND $7::text IS NOT NULL
       AND (t.opened_by IS NULL OR t.opened_by = 'operator')
     ORDER BY c.chunk_embedding <=> $7::text::public.vector
     LIMIT 200)
), semantic AS (
    SELECT reference, score
    FROM (SELECT reference, max(score) AS score, max(max(score)) OVER () AS best
          FROM semantic_chunks
          WHERE ($3::text IS NULL OR split_part(reference, ':', 1)
                = CASE WHEN $3::text IN ('memory', 'skill') THEN 'artifact' ELSE $3::text END)
            AND NOT EXISTS (SELECT 1 FROM own_refs o WHERE o.reference = semantic_chunks.reference)
          GROUP BY reference) per_reference
    WHERE score >= GREATEST(($10::float8[])[1], best - ($10::float8[])[2])
), idents AS (
    SELECT h.txt, h.pos, h.is_hash,
           '(^|[^[:alnum:]])' || regexp_replace(h.txt, '([^[:alnum:]])', '\\\1', 'g')
           || CASE WHEN h.is_hash THEN '($|[^[:alnum:]]|[0-9a-fA-F])' ELSE '($|[^[:alnum:]])' END AS pattern
    FROM (SELECT u.txt, u.pos,
                 u.txt ~ '^[0-9a-fA-F]{7,40}$' AND u.txt ~ '[0-9]' AND u.txt ~ '[a-fA-F]' AS is_hash
            FROM unnest($14::text[]) WITH ORDINALITY AS u(txt, pos)) h
), ident_tsq AS (
    SELECT string_agg('(' || i.tsq::text || ')', ' | ')::tsquery AS tsq
    FROM (SELECT CASE WHEN d.is_hash THEN to_tsquery('english', lower(d.txt) || ':*')
                      ELSE phraseto_tsquery('english', d.txt) END AS tsq
            FROM idents d) i
    WHERE numnode(i.tsq) > 0
), q AS (
    SELECT base.tsq,
           CASE WHEN ident_tsq.tsq IS NULL THEN base.tsq ELSE base.tsq || ident_tsq.tsq END AS match_tsq,
           ident_tsq.tsq AS ident_tsq
      FROM (SELECT CASE WHEN ($11::bool[])[1] AND strpos(w::text, '!') = 0 AND numnode(w) > 0
                        THEN replace(w::text, ' & ', ' | ')::tsquery ELSE w END AS tsq
              FROM websearch_to_tsquery('english', $2) w) base
     CROSS JOIN ident_tsq
), superseded AS (
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
     WHERE sl.project_id = $1
), bound AS (
    -- The page needs the best N = limit + offset rows. A row beyond position M in both the words and the
    -- meaning list scores below 2/(60 + M), under half of the worst possible score of the N-th row (half,
    -- because rows ranked lower count at 0.5), so only the best M of each list, and the newest M of the
    -- rows without any score (an empty query), are kept.
    SELECT 4 * ($5::bigint + $6::bigint) + 180 AS m
), candidates AS (
    SELECT COALESCE(r.search_tsv @@ q.ident_tsq, false) AS is_ident,
           'artifact:' || r.id AS reference, 'artifact' AS source_kind, r.title,
           r.status, d.relative_path AS path, r.observed_at AS updated_at,
           ts_rank_cd(r.search_tsv, q.tsq) AS rank, COALESCE(sem.score, 0) AS semantic_rank,
           jsonb_build_object('document_uuid', d.document_uuid, 'revision', r.id,
             'location_uuid', s.location_uuid, 'last_scan_at', s.last_scan_at,
             'deleted', d.deleted_at IS NOT NULL, 'metadata', r.metadata,
             'freshness', sf.freshness) AS provenance,
           r.body AS body, NULL::jsonb AS author,
           s.name || ':' || d.relative_path AS dedupe_key,
           NULL::bigint AS turn_id, NULL::text AS turn_prompt, false AS answered
      FROM (SELECT h.id FROM memory.knowledge_revisions h
             WHERE $2 <> '' AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT split_part(s2.reference, ':', 2)::bigint FROM semantic s2
             WHERE s2.reference LIKE 'artifact:%'
            UNION
            (SELECT d0.current_revision FROM memory.knowledge_documents d0
              JOIN memory.knowledge_sources s0 ON s0.source_uuid = d0.source_uuid
              JOIN projects.locations l0 ON l0.location_uuid = s0.location_uuid
              JOIN memory.knowledge_revisions r0 ON r0.id = d0.current_revision
             WHERE $2 = '' AND l0.project_id = $1
             ORDER BY r0.observed_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.knowledge_revisions r ON r.id = hit.id
      JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid AND d.current_revision = r.id
      JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
      JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
      JOIN source_freshness sf ON sf.source_uuid = s.source_uuid
      CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'artifact:' || r.id
     WHERE loc.project_id = $1 AND ($2 = '' OR r.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
       AND ($3::text IS NULL OR $3::text = 'artifact')
       AND s.name NOT IN ('claude-memory', 'skills')
       AND ($4 OR (d.deleted_at IS NULL AND r.status NOT IN ('superseded', 'archived', 'rejected')))
    UNION ALL
    SELECT COALESCE(r.search_tsv @@ q.ident_tsq, false),
           'artifact:' || r.id AS reference,
           CASE s.name WHEN 'claude-memory' THEN 'memory' ELSE 'skill' END AS source_kind, r.title,
           r.status, d.relative_path AS path, r.observed_at AS updated_at,
           ts_rank_cd(r.search_tsv, q.tsq) AS rank, COALESCE(sem.score, 0) AS semantic_rank,
           jsonb_build_object('document_uuid', d.document_uuid, 'revision', r.id,
             'location_uuid', s.location_uuid, 'last_scan_at', s.last_scan_at,
             'deleted', d.deleted_at IS NOT NULL, 'metadata', r.metadata,
             'freshness', sf.freshness) AS provenance,
           r.body AS body, NULL::jsonb AS author,
           s.name || ':' || d.relative_path AS dedupe_key,
           NULL::bigint AS turn_id, NULL::text AS turn_prompt, false AS answered
      FROM (SELECT h.id FROM memory.knowledge_revisions h
             WHERE $2 <> '' AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT split_part(s2.reference, ':', 2)::bigint FROM semantic s2
             WHERE s2.reference LIKE 'artifact:%'
            UNION
            (SELECT d0.current_revision FROM memory.knowledge_documents d0
              JOIN memory.knowledge_sources s0 ON s0.source_uuid = d0.source_uuid
              JOIN projects.locations l0 ON l0.location_uuid = s0.location_uuid
              JOIN memory.knowledge_revisions r0 ON r0.id = d0.current_revision
             WHERE $2 = '' AND l0.project_id = $1
             ORDER BY r0.observed_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.knowledge_revisions r ON r.id = hit.id
      JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid AND d.current_revision = r.id
      JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
      JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
      JOIN source_freshness sf ON sf.source_uuid = s.source_uuid
      CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'artifact:' || r.id
     WHERE loc.project_id = $1 AND ($2 = '' OR r.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
       AND ($3::text IS NULL OR $3::text IN ('memory', 'skill'))
       AND s.name IN ('claude-memory', 'skills')
       AND ($4 OR (d.deleted_at IS NULL AND r.status NOT IN ('superseded', 'archived', 'rejected')))
    UNION ALL
    SELECT COALESCE(to_tsvector('english', l.text) @@ q.ident_tsq, false),
           'lesson:' || l.uuid, 'lesson', btrim(split_part(btrim(l.text), E'\n', 1)), st.code,
           NULL, l.created_at,
           ts_rank_cd(to_tsvector('english', l.text), q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('lesson_id', l.id, 'lesson_kind', k.code, 'topic', l.topic,
             'source_turn_id', l.source_turn_id),
           l.text, NULL::jsonb, 'lesson:' || l.uuid, NULL, NULL, false
      FROM (SELECT h.id FROM memory.lessons h
             WHERE $2 <> '' AND h.project_id = $1 AND to_tsvector('english', h.text) @@ (SELECT match_tsq FROM q)
            UNION
            SELECT l0.id FROM semantic s2 JOIN memory.lessons l0 ON l0.uuid = substr(s2.reference, 8)::uuid
             WHERE s2.reference LIKE 'lesson:%'
            UNION
            SELECT h.id FROM memory.lessons h WHERE $2 = '' AND h.project_id = $1) hit
      JOIN memory.lessons l ON l.id = hit.id
      JOIN memory.lesson_kinds k ON k.id = l.kind_id
      JOIN memory.lesson_stages st ON st.id = l.stage_id
      LEFT JOIN superseded sup ON sup.to_lesson_id = l.id CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'lesson:' || l.uuid
     WHERE l.project_id = $1 AND ($3::text IS NULL OR $3::text = 'lesson')
       AND ($2 = '' OR to_tsvector('english', l.text) @@ q.match_tsq OR sem.score IS NOT NULL)
       AND ($4 OR (l.archived_at IS NULL AND (l.expires_at IS NULL OR l.expires_at > now())
         AND sup.to_lesson_id IS NULL))
    UNION ALL
    SELECT COALESCE(n.search_tsv @@ q.ident_tsq, false),
           'note:' || n.note_uuid, 'note', n.title, COALESCE(n.status, 'unknown'), NULL,
           n.updated_at, ts_rank_cd(n.search_tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('session_uuid', ss.session_uuid, 'kind', n.kind, 'tags', n.tags),
           n.body, jsonb_build_object('role', 'session', 'session_uuid', ss.session_uuid,
             'session_title', ss.title),
           'note:' || n.note_uuid, NULL, NULL, false
      FROM (SELECT h.id FROM workspace.session_notes h
             WHERE $2 <> '' AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT n0.id FROM semantic s JOIN workspace.session_notes n0 ON n0.note_uuid = substr(s.reference, 6)::uuid
             WHERE s.reference LIKE 'note:%'
            UNION
            (SELECT h.id FROM workspace.session_notes h JOIN sessions.sessions hs ON hs.id = h.session_id
             WHERE $2 = '' AND hs.project_id = $1
             ORDER BY h.updated_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN workspace.session_notes n ON n.id = hit.id
      JOIN sessions.sessions ss ON ss.id = n.session_id CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'note:' || n.note_uuid
     WHERE ss.project_id = $1 AND ($3::text IS NULL OR $3::text = 'note')
       AND ($2 = '' OR n.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
       AND ($4 OR COALESCE(n.status, '') NOT IN ('superseded', 'archived', 'rejected'))
    UNION ALL
    SELECT COALESCE(t.search_tsv @@ q.ident_tsq, false),
           'turn:' || t.id, 'turn', COALESCE(t.user_prompt, ''), 'historical', NULL,
           t.created_at, ts_rank_cd(t.search_tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('session_uuid', ss.session_uuid),
           t.user_prompt, jsonb_build_object('role', 'operator', 'session_uuid', ss.session_uuid,
             'session_title', ss.title),
           'turn:' || t.id, t.id, t.user_prompt, COALESCE(octet_length(t.final_assistant_text), 0) > 0
      FROM (SELECT h.id FROM memory.operator_turns h
             WHERE $2 <> '' AND h.project_id = $1 AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT split_part(s.reference, ':', 2)::bigint FROM semantic s
             WHERE s.reference LIKE 'turn:%'
            UNION
            (SELECT h.id FROM memory.operator_turns h WHERE $2 = '' AND h.project_id = $1
             ORDER BY h.created_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.operator_turns t ON t.id = hit.id
      JOIN sessions.sessions ss ON ss.id = t.session_id CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'turn:' || t.id
     WHERE t.project_id = $1 AND ($3::text IS NULL OR $3::text = 'turn')
       AND ($2 = '' OR t.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
       AND (t.opened_by IS NULL OR t.opened_by = 'operator')
    UNION ALL
    SELECT COALESCE(hit.is_ident, false),
           'thread:' || th.thread_uuid, 'thread', th.subject,
           CASE WHEN th.resolved_at IS NULL THEN 'open' ELSE 'resolved' END, NULL,
           th.last_message_at, COALESCE(hit.rank, 0), COALESCE(sem.score, 0),
           jsonb_build_object('thread_uuid', th.thread_uuid, 'fleet_uuid', th.fleet_uuid),
           th.subject, NULL::jsonb, 'thread:' || th.thread_uuid, NULL, NULL, false
      FROM mailbox.threads th CROSS JOIN q
      LEFT JOIN (SELECT m.thread_id, max(ts_rank_cd(m.search_tsv, q.tsq)) AS rank,
                        bool_or(COALESCE(m.search_tsv @@ q.ident_tsq, false)) AS is_ident
                   FROM mailbox.messages m JOIN mailbox.threads pt ON pt.id = m.thread_id CROSS JOIN q
                  WHERE $2 <> '' AND pt.project_id = $1 AND m.search_tsv @@ q.match_tsq
                  GROUP BY m.thread_id) hit ON hit.thread_id = th.id
      LEFT JOIN semantic sem ON sem.reference = 'thread:' || th.thread_uuid
     WHERE th.project_id = $1 AND ($3::text IS NULL OR $3::text = 'thread')
       AND ($2 = '' OR hit.thread_id IS NOT NULL OR sem.score IS NOT NULL)
    UNION ALL
    SELECT COALESCE(doc.tsv @@ q.ident_tsq, false),
           'work_thread:' || wt.slug, 'work_thread', COALESCE(wt.title, wt.slug),
           COALESCE(wt.status, 'unknown'), NULL,
           CASE WHEN wlast.entry_date IS NOT NULL THEN wlast.entry_date::timestamp AT TIME ZONE 'UTC'
                ELSE wt.updated_at END,
           ts_rank_cd(doc.tsv, q.tsq), 0::double precision,
           jsonb_build_object('thread_id', wt.id, 'slug', wt.slug, 'goal', wt.goal),
           doc.txt, NULL::jsonb, 'work_thread:' || wt.slug, NULL, NULL, false
      FROM work.threads wt LEFT JOIN work.thread_state wts ON wts.thread_id = wt.id CROSS JOIN q
      LEFT JOIN LATERAL (SELECT max(wl.entry_date) AS entry_date FROM work.thread_log wl
                          WHERE wl.thread_id = wt.id AND wt.updated_at = wt.parsed_at) wlast ON true
     CROSS JOIN LATERAL (SELECT concat_ws(E'\n', wt.title, wt.slug, wt.goal,
              wts.tldr, wts.where_we_are, wts.next_up, wts.blockers, wts.open_questions,
              wts.decisions) AS txt) txt
     CROSS JOIN LATERAL (SELECT txt.txt,
              COALESCE(wt.search_tsv, ''::tsvector) || COALESCE(wts.search_tsv, ''::tsvector) AS tsv) doc
     WHERE wt.project_id = $1 AND ($3::text IS NULL OR $3::text = 'work_thread')
       AND ($2 = '' OR doc.tsv @@ q.match_tsq)
    UNION ALL
    SELECT COALESCE(doc.tsv @@ q.ident_tsq, false),
           'thread_entry:' || l.id, 'thread_entry',
           COALESCE(NULLIF(btrim(split_part(btrim(l.summary), E'\n', 1)), ''), l.kind),
           l.kind, NULL,
           CASE WHEN l.entry_date < (l.created_at AT TIME ZONE 'UTC')::date
                THEN l.entry_date::timestamp AT TIME ZONE 'UTC' ELSE l.created_at END,
           ts_rank_cd(doc.tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('slug', wt.slug, 'entry_kind', l.kind, 'entry_date', l.entry_date,
             'commit_sha', l.commit_sha, 'session_uuid', l.session_uuid),
           doc.txt, CASE WHEN l.session_uuid IS NOT NULL THEN jsonb_build_object('role', 'session',
             'session_uuid', l.session_uuid, 'session_title', ws.title) END,
           'thread_entry:' || l.id, NULL, NULL, false
      FROM (SELECT h.id FROM work.thread_log h
             WHERE $2 <> '' AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT split_part(s.reference, ':', 2)::bigint FROM semantic s
             WHERE s.reference LIKE 'thread_entry:%'
            UNION
            (SELECT h.id FROM work.thread_log h JOIN work.threads ht ON ht.id = h.thread_id
             WHERE $2 = '' AND ht.project_id = $1
             ORDER BY h.created_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN work.thread_log l ON l.id = hit.id JOIN work.threads wt ON wt.id = l.thread_id
      LEFT JOIN sessions.sessions ws ON ws.session_uuid = l.session_uuid CROSS JOIN q
     CROSS JOIN LATERAL (SELECT concat_ws(E'\n\n', l.summary, l.body) AS txt, l.search_tsv AS tsv) doc
      LEFT JOIN semantic sem ON sem.reference = 'thread_entry:' || l.id
     WHERE wt.project_id = $1 AND ($3::text IS NULL OR $3::text = 'thread_entry')
       AND ($2 = '' OR doc.tsv @@ q.match_tsq OR sem.score IS NOT NULL)
       AND ($4 OR l.kind IS DISTINCT FROM 'state-replaced')
    UNION ALL
    SELECT COALESCE(doc.tsv @@ q.ident_tsq, false),
           'question_answer:' || qa.question_uuid, 'question_answer',
           btrim(split_part(btrim(qa.question), E'\n', 1)), qa.state, NULL, qa.settled_at,
           ts_rank_cd(doc.tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('question_uuid', qa.question_uuid, 'run_id', qa.run_id,
             'to_task', qa.to_task, 'asker_session_uuid', asker.session_uuid),
           doc.txt, jsonb_build_object('role', 'session', 'session_uuid', settler.session_uuid,
             'session_title', settler.title),
           'question_answer:' || qa.question_uuid, NULL, NULL, false
      FROM (SELECT h.id FROM workspace.questions h
             WHERE $2 <> '' AND to_tsvector('english', h.question || ' ' || COALESCE(h.answer, ''))
                   @@ (SELECT match_tsq FROM q)
            UNION
            SELECT h.id FROM workspace.questions h JOIN semantic s
                ON s.reference = 'question_answer:' || h.question_uuid
             WHERE s.reference LIKE 'question_answer:%'
            UNION
            SELECT h.id FROM workspace.questions h WHERE $2 = '' AND h.project_id = $1) hit
      JOIN workspace.questions qa ON qa.id = hit.id
      LEFT JOIN sessions.sessions asker ON asker.id = qa.asker_session_id
      LEFT JOIN sessions.sessions settler ON settler.id = qa.settled_by_session_id CROSS JOIN q
     CROSS JOIN LATERAL (SELECT 'Question: ' || qa.question || E'\n\nAnswer: ' || qa.answer AS txt,
              to_tsvector('english', qa.question || ' ' || COALESCE(qa.answer, '')) AS tsv) doc
      LEFT JOIN semantic sem ON sem.reference = 'question_answer:' || qa.question_uuid
     WHERE qa.project_id = $1 AND qa.answer IS NOT NULL AND ($3::text IS NULL OR $3::text = 'question_answer')
       AND ($2 = '' OR doc.tsv @@ q.match_tsq OR sem.score IS NOT NULL)
    UNION ALL
    SELECT COALESCE(vt.tsv @@ q.ident_tsq, false),
           vk.kind || ':' || v.id, vk.kind,
           CASE vk.kind WHEN 'unmet_ask' THEN btrim(split_part(btrim(v.unmet_ask), E'\n', 1))
                ELSE btrim(split_part(btrim(COALESCE(t.user_prompt, '')), E'\n', 1)) END,
           v.reaction, NULL, v.created_at,
           ts_rank_cd(vt.tsv, q.tsq),
           COALESCE(sem.score, 0),
           jsonb_build_object('verdict_id', v.id, 'turn_reference', 'turn:' || v.operator_turn_id,
             'reaction', v.reaction, 'session_uuid', vs.session_uuid),
           concat_ws(E'\n\n', NULLIF(btrim(COALESCE(v.unmet_ask, '')), ''), v.rationale),
           jsonb_build_object('role', 'judge', 'session_uuid', vs.session_uuid,
             'session_title', vs.title),
           vk.kind || ':' || v.id, v.operator_turn_id, t.user_prompt, false
      FROM (SELECT h.id FROM memory.turn_verdicts h
             WHERE $2 <> '' AND h.mode = 'live'
               AND to_tsvector('english', h.unmet_ask || ' ' || h.rationale) @@ (SELECT match_tsq FROM q)
            UNION
            SELECT split_part(s.reference, ':', 2)::bigint FROM semantic s
             WHERE s.reference LIKE 'unmet_ask:%' OR s.reference LIKE 'correction:%'
            UNION
            (SELECT h.id FROM memory.live_turn_verdicts_v h WHERE $2 = '' AND h.project_id = $1
             ORDER BY h.created_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.live_turn_verdicts_v v ON v.id = hit.id
      JOIN memory.operator_turns t ON t.id = v.operator_turn_id
      LEFT JOIN sessions.sessions vs ON vs.id = v.session_id
     CROSS JOIN LATERAL (SELECT to_tsvector('english', v.unmet_ask || ' ' || v.rationale) AS tsv OFFSET 0) vt
     CROSS JOIN LATERAL (VALUES ('unmet_ask'), ('correction')) vk(kind) CROSS JOIN q
      LEFT JOIN semantic sem ON sem.reference = vk.kind || ':' || v.id
     WHERE v.project_id = $1 AND ($3::text IS NULL OR $3::text IN ('unmet_ask', 'correction'))
       AND CASE vk.kind WHEN 'unmet_ask' THEN btrim(COALESCE(v.unmet_ask, '')) <> ''
                ELSE v.reaction IN ('corrected', 'redirected') END
       AND ($2 = '' OR vt.tsv @@ q.match_tsq
            OR sem.score IS NOT NULL)
    UNION ALL
    SELECT COALESCE(to_tsvector('english', memory.session_learnings_text(k.learnings)) @@ q.ident_tsq, false),
           'learning:' || k.session_id, 'learning', btrim(split_part(btrim(k.learnings[1]), E'\n', 1)),
           'historical', NULL, k.generated_at,
           ts_rank_cd(to_tsvector('english', memory.session_learnings_text(k.learnings)), q.tsq),
           COALESCE(sem.score, 0),
           jsonb_build_object('session_uuid', k.session_id, 'topics', k.topics),
           memory.session_learnings_text(k.learnings),
           jsonb_build_object('role', 'judge', 'session_uuid', k.session_id, 'session_title', ls.title),
           'learning:' || k.session_id, NULL, NULL, false
      FROM (SELECT h.session_id FROM memory.session_knowledge h
             WHERE $2 <> '' AND to_tsvector('english', memory.session_learnings_text(h.learnings))
                   @@ (SELECT match_tsq FROM q)
            UNION
            SELECT h.session_id FROM memory.session_knowledge h JOIN semantic s
                ON s.reference = 'learning:' || h.session_id
             WHERE s.reference LIKE 'learning:%'
            UNION
            (SELECT h.session_id FROM memory.session_knowledge h
              JOIN sessions.sessions hs ON hs.session_uuid = h.session_id
             WHERE $2 = '' AND hs.project_id = $1 AND cardinality(h.learnings) > 0
             ORDER BY h.generated_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.session_knowledge k ON k.session_id = hit.session_id
      JOIN sessions.sessions ls ON ls.session_uuid = k.session_id
     CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'learning:' || k.session_id
     WHERE ls.project_id = $1 AND cardinality(k.learnings) > 0 AND ($3::text IS NULL OR $3::text = 'learning')
       AND ($2 = '' OR to_tsvector('english', memory.session_learnings_text(k.learnings)) @@ q.match_tsq
            OR sem.score IS NOT NULL)
    UNION ALL
    SELECT COALESCE(to_tsvector('english', ss.overview) @@ q.ident_tsq, false),
           'session:' || ss.session_uuid, 'session',
           COALESCE(NULLIF(btrim(ss.title), ''), left(regexp_replace(btrim(ss.overview), '\s+', ' ', 'g'), 80)),
           'historical', NULL, COALESCE(ss.overview_generated_at, ss.last_seen),
           ts_rank_cd(to_tsvector('english', ss.overview), q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('session_uuid', ss.session_uuid, 'topics', COALESCE(k.topics, '{}'::text[])),
           ss.overview || CASE WHEN cardinality(k.topics) > 0
                               THEN E'\n\nTopics: ' || array_to_string(k.topics, ', ') ELSE '' END,
           jsonb_build_object('role', 'judge', 'session_uuid', ss.session_uuid, 'session_title', ss.title),
           'session:' || ss.session_uuid, NULL, NULL, false
      FROM (SELECT h.id FROM sessions.sessions h
             WHERE $2 <> '' AND h.project_id = $1 AND to_tsvector('english', h.overview) @@ (SELECT match_tsq FROM q)
            UNION
            SELECT s0.id FROM semantic s2 JOIN sessions.sessions s0 ON s0.session_uuid = substr(s2.reference, 9)::uuid
             WHERE s2.reference LIKE 'session:%'
            UNION
            (SELECT h.id FROM sessions.sessions h
             WHERE $2 = '' AND h.project_id = $1 AND btrim(coalesce(h.overview, '')) <> ''
             ORDER BY COALESCE(h.overview_generated_at, h.last_seen) DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN sessions.sessions ss ON ss.id = hit.id
      LEFT JOIN memory.session_knowledge k ON k.session_id = ss.session_uuid
     CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'session:' || ss.session_uuid
     WHERE ss.project_id = $1 AND btrim(coalesce(ss.overview, '')) <> '' AND ($3::text IS NULL OR $3::text = 'session')
       AND ($2 = '' OR to_tsvector('english', ss.overview) @@ q.match_tsq OR sem.score IS NOT NULL)
    UNION ALL
    SELECT COALESCE(m.search_tsv @@ q.ident_tsq, false),
           'mail:' || m.message_uuid, 'mail', btrim(split_part(btrim(m.body), E'\n', 1)),
           CASE WHEN m.dead_lettered_at IS NOT NULL THEN 'dead_lettered'
                WHEN m.consumed_at IS NOT NULL THEN 'consumed'
                WHEN m.sent_at IS NOT NULL OR m.channel_delivered_at IS NOT NULL THEN 'delivered'
                ELSE 'queued' END,
           NULL, m.queued_at, ts_rank_cd(m.search_tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('message_uuid', m.message_uuid, 'thread_uuid', mt.thread_uuid,
             'to_session_uuid', mto.session_uuid, 'from_session_uuid', mf.session_uuid),
           m.body, jsonb_build_object('role', 'session', 'session_uuid', mf.session_uuid,
             'session_title', mf.title),
           'mail:' || m.message_uuid, NULL, NULL, false
      FROM (SELECT h.id FROM mailbox.messages h
             WHERE $2 <> '' AND h.from_session_id IS NOT NULL AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT h.id FROM mailbox.messages h JOIN semantic s
                ON s.reference = 'mail:' || h.message_uuid
             WHERE s.reference LIKE 'mail:%'
            UNION
            (SELECT h.id FROM mailbox.messages h WHERE $2 = '' AND h.from_session_id IS NOT NULL
             ORDER BY h.queued_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN mailbox.messages m ON m.id = hit.id
      JOIN sessions.sessions mf ON mf.id = m.from_session_id
      LEFT JOIN mailbox.threads mt ON mt.id = m.thread_id
      LEFT JOIN sessions.sessions mto ON mto.id = m.to_session_id CROSS JOIN q
      LEFT JOIN semantic sem ON sem.reference = 'mail:' || m.message_uuid
     WHERE COALESCE(mt.project_id, mf.project_id) = $1 AND ($3::text IS NULL OR $3::text = 'mail')
       AND ($2 = '' OR m.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
    UNION ALL
    SELECT COALESCE(si.search_tsv @@ q.ident_tsq, false),
           seat_ref.reference, 'seat', btrim(split_part(btrim(si.text), E'\n', 1)), si.item_kind,
           NULL, si.at, ts_rank_cd(si.search_tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('tier', 'seat', 'item_kind', si.item_kind, 'run_id', si.run_id,
             'seat', si.seat, 'task', si.task, 'session_uuid', si.session_uuid,
             'from_session', si.from_session, 'from_role', si.from_role,
             'repeat_count', si.repeat_count, 'report_filed', si.report_filed,
             'verdict_accepted', si.verdict_accepted,
             'framing', 'seat material: written by or for a fleet seat or worker session, not by the operator'),
           si.text,
           jsonb_build_object('role', si.author, 'session_uuid', si.session_uuid,
             'session_title', sts.title, 'seat', si.seat, 'run_id', si.run_id, 'task', si.task),
           seat_ref.reference, NULL, NULL, false
      FROM (SELECT s0.* FROM memory.seat_memory_items s0
             WHERE s0.project_id = $1
             ORDER BY CASE WHEN $2 = '' THEN s0.at END DESC NULLS LAST
             LIMIT CASE WHEN $2 = '' THEN (SELECT m FROM bound) END) si
      LEFT JOIN sessions.sessions sts ON sts.id = si.session_id CROSS JOIN q
     CROSS JOIN LATERAL (SELECT CASE WHEN si.source_ref LIKE 'entry:%'
                                     THEN 'thread_entry:' || substr(si.source_ref, 7)
                                     ELSE si.source_ref END AS reference) seat_ref
      LEFT JOIN semantic sem ON sem.reference = seat_ref.reference
     WHERE si.project_id = $1 AND ($3::text IS NULL OR $3::text = 'seat') AND ($2 = '' OR si.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
), cheap AS MATERIALIZED (
    SELECT c.*, row_number() OVER () AS rid
      FROM candidates c
      LEFT JOIN own_refs o ON o.reference = c.reference
     WHERE ($3::text IS NULL OR c.source_kind = $3)
       AND o.reference IS NULL
), positioned AS (
    SELECT p.rid,
           row_number() OVER (ORDER BY p.rank DESC, p.reference) AS lex_pos,
           row_number() OVER (ORDER BY p.semantic_rank DESC, p.reference) AS sem_pos,
           row_number() OVER (PARTITION BY (p.rank > 0 OR p.semantic_rank > 0)
                              ORDER BY p.updated_at DESC NULLS LAST, p.reference) AS new_pos,
           (p.rank > 0) AS has_lex, (p.semantic_rank > 0) AS has_sem
      FROM cheap p
), bounded AS (
    SELECT c.*, p.lex_pos, p.sem_pos
      FROM cheap c
      JOIN positioned p ON p.rid = c.rid
     WHERE (p.has_lex AND p.lex_pos <= (SELECT m FROM bound))
        OR (p.has_sem AND p.sem_pos <= (SELECT m FROM bound))
        OR (NOT p.has_lex AND NOT p.has_sem AND p.new_pos <= (SELECT m FROM bound))
        OR c.is_ident
), eligible AS (
    SELECT c.*
      FROM bounded c
      LEFT JOIN LATERAL (
          SELECT nx.user_prompt AS next_prompt
            FROM memory.operator_turns cur
            JOIN memory.operator_turns nx ON nx.session_id = cur.session_id AND nx.id > cur.id
           WHERE cur.id = c.turn_id AND c.source_kind = 'turn' AND NOT c.answered AND NOT $4
             AND btrim(COALESCE(c.turn_prompt, '')) <> ''
           ORDER BY nx.id LIMIT 1) resent ON true
     WHERE (resent.next_prompt IS NULL OR NOT starts_with(resent.next_prompt, btrim(c.turn_prompt)))
), deduped AS (
    SELECT DISTINCT ON (dedupe_key) *
      FROM (SELECT *, GREATEST(1, count(*) FILTER (WHERE source_kind <> 'seat')
                                    OVER (PARTITION BY dedupe_key)) AS copies
              FROM eligible) c
     ORDER BY dedupe_key, source_kind = 'seat' DESC, semantic_rank DESC, rank DESC,
              updated_at DESC NULLS LAST, reference
), keyed AS (
    SELECT d.*,
           CASE WHEN btrim(COALESCE(d.turn_prompt, '')) <> '' THEN 'p:' || md5(lower(btrim(d.turn_prompt)))
                WHEN d.source_kind IN ('artifact', 'memory', 'skill')
                    -- content_hash is the sha256 of the body, written with the revision. The blank test reads only
                    -- the first 64 characters unless they are all spaces.
                    THEN CASE WHEN kr.content_hash IS NOT NULL
                               AND CASE WHEN btrim(substr(kr.body, 1, 64)) <> '' THEN true
                                        ELSE btrim(kr.body) <> '' END
                              THEN 'b:' || kr.content_hash ELSE d.reference END
                WHEN btrim(COALESCE(d.body, '')) <> ''
                    THEN 'b:' || encode(sha256(convert_to(d.body, 'UTF8')), 'hex')
                ELSE d.reference END AS slot_key,
           d.source_kind IN ('seat', 'mail', 'thread', 'session')
           OR (d.source_kind = 'note' AND d.provenance ->> 'kind' = 'brief')
           OR COALESCE(ltrim(d.turn_prompt) ^@ ANY ($13::text[]), false) AS machine,
           d.source_kind IN ('seat', 'thread')
           OR (d.source_kind = 'note' AND d.provenance ->> 'kind' = 'brief')
           OR COALESCE(ltrim(d.turn_prompt) ^@ ANY ($13::text[]), false) AS ranked_lower
      FROM deduped d
      LEFT JOIN memory.knowledge_revisions kr
        ON kr.id = CASE WHEN d.source_kind IN ('artifact', 'memory', 'skill')
                        THEN substr(d.reference, 10)::bigint END
), slotted AS (
    SELECT k.*, row_number() OVER slot AS slot_pos,
           array_agg(k.reference) OVER (slot ROWS BETWEEN 1 FOLLOWING AND 10 FOLLOWING) AS also,
           count(*) OVER (PARTITION BY k.slot_key) - 1 AS also_count
      FROM keyed k
    WINDOW slot AS (PARTITION BY k.slot_key
                    ORDER BY k.semantic_rank DESC, k.rank DESC,
                             (k.source_kind = 'turn' AND k.answered) DESC,
                             k.updated_at DESC NULLS LAST, k.reference)
), winners AS (
    SELECT s.*, (rank > 0 AND lex_pos <= (SELECT m FROM bound)) AS in_lex,
           (semantic_rank > 0 AND sem_pos <= (SELECT m FROM bound)) AS in_sem
    FROM slotted s
    WHERE slot_pos = 1
), placed AS (
    -- A row kept because it is in one list but beyond M in the other is placed after the winners of that list,
    -- by its distance beyond M among all candidates, so that bounding the other list does not lift it.
    SELECT w.*,
           row_number() OVER (PARTITION BY in_lex ORDER BY rank DESC, reference) AS lex_rn,
           row_number() OVER (PARTITION BY in_sem ORDER BY semantic_rank DESC, reference) AS sem_rn,
           count(*) FILTER (WHERE in_lex) OVER () AS lex_w,
           count(*) FILTER (WHERE in_sem) OVER () AS sem_w
    FROM winners w
), ranked AS (
    SELECT p.*, ih.hits AS identifier_hits,
      (CASE WHEN rank > 0 THEN 1.0 / (60 + CASE WHEN in_lex THEN lex_rn
                       ELSE lex_w + GREATEST(lex_pos - (SELECT m FROM bound), 1) END) ELSE 0 END
      + CASE WHEN semantic_rank > 0 THEN 1.0 / (60 + CASE WHEN in_sem THEN sem_rn
                       ELSE sem_w + GREATEST(sem_pos - (SELECT m FROM bound), 1) END) ELSE 0 END)
      * CASE WHEN ranked_lower THEN 0.5 ELSE 1 END AS fused_rank
    FROM placed p
    CROSS JOIN LATERAL (
        SELECT count(*) AS hits FROM idents i
         WHERE COALESCE(p.title, '') ~* i.pattern OR COALESCE(p.body, '') ~* i.pattern) ih
), page AS (
    SELECT * FROM ranked
    ORDER BY identifier_hits DESC, fused_rank DESC, updated_at DESC, reference LIMIT $5 OFFSET $6
)
SELECT jsonb_build_object('reference' , p.reference, 'source_kind', p.source_kind,
    'title', p.title, 'status', p.status, 'path', p.path, 'updated_at', p.updated_at,
    'author', p.author, 'copies', p.copies,
    'also', COALESCE(to_jsonb(p.also), '[]'::jsonb), 'also_count', p.also_count,
    'excerpt', substr(COALESCE(p.body, ''),
                      GREATEST(1, CASE WHEN found.m IS NULL
                                       THEN strpos(lower(COALESCE(p.body, '')),
                                                   lower(split_part(btrim($2), ' ', 1)))
                                       ELSE length(COALESCE((found.m)[1], '')) + length(COALESCE((found.m)[2], '')) + 1
                                  END - 100), 300),
    'rank', p.fused_rank, 'lexical_rank', p.rank, 'semantic_rank', p.semantic_rank,
    'identifier_hits', p.identifier_hits,
    'provenance', p.provenance
                  || CASE WHEN p.machine THEN jsonb_build_object('written_by', 'machine') ELSE '{}'::jsonb END
                  || CASE WHEN p.ranked_lower THEN jsonb_build_object('ranked_lower', true) ELSE '{}'::jsonb END
    ) AS "item!"
FROM page p
LEFT JOIN LATERAL (
    SELECT regexp_match(p.body, '^(.*?)' || i.pattern, 'i') AS m
      FROM idents i
     WHERE COALESCE(p.body, '') ~* i.pattern
     ORDER BY i.pos LIMIT 1) found ON true
ORDER BY p.identifier_hits DESC, p.fused_rank DESC, p.updated_at DESC, p.reference
