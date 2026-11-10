WITH candidates AS (
    SELECT s.source_kind, s.source_id, s.reference, s.project_id,
           s.source_updated_at, s.source_seq, s.source_hash,
           row_number() OVER (PARTITION BY s.project_id
                              ORDER BY e.next_attempt_at NULLS FIRST, s.source_kind, s.source_id) AS project_rank
      FROM memory.knowledge_sources_with_sessions_v s
      LEFT JOIN memory.knowledge_embedding_state e
        ON e.source_kind = s.source_kind AND e.source_id = s.source_id
     WHERE e.id IS NULL OR e.model_key <> $1
        OR e.source_updated_at IS DISTINCT FROM s.source_updated_at
        OR e.source_seq IS DISTINCT FROM s.source_seq
        OR e.source_hash IS DISTINCT FROM s.source_hash
        OR (e.completed_at IS NULL AND e.next_attempt_at <= now()
            AND (e.attempts < 10 OR e.last_error LIKE '[transient]%'))
), picked AS MATERIALIZED (
    SELECT source_kind, source_id, reference, project_id,
           source_updated_at, source_seq, source_hash, project_rank
      FROM candidates
     ORDER BY project_rank, project_id, source_kind, source_id
     LIMIT $2
), bodies AS (
    SELECT 'artifact'::text AS source_kind, r.id AS source_id, r.title || E'\n\n' || r.body AS body
      FROM memory.knowledge_revisions r
     WHERE r.id = ANY(ARRAY(SELECT source_id FROM picked WHERE source_kind = 'artifact'))
    UNION ALL
    SELECT 'lesson', l.id, l.text
      FROM memory.lessons l
     WHERE l.id = ANY(ARRAY(SELECT source_id FROM picked WHERE source_kind = 'lesson'))
    UNION ALL
    SELECT 'note', n.id, n.title || E'\n\n' || n.body
      FROM workspace.session_notes n
     WHERE n.id = ANY(ARRAY(SELECT source_id FROM picked WHERE source_kind = 'note'))
    UNION ALL
    SELECT 'thread', tx.thread_id, tx.body
      FROM mailbox.thread_text_v tx
     WHERE tx.thread_id = ANY(ARRAY(SELECT source_id FROM picked WHERE source_kind = 'thread'))
    UNION ALL
    SELECT 'thread_entry', l.id,
           concat_ws(E'\n\n', 'Work thread ' || wt.slug || ': ' || l.kind, l.summary, l.body)
      FROM work.thread_log l JOIN work.threads wt ON wt.id = l.thread_id
     WHERE l.id = ANY(ARRAY(SELECT source_id FROM picked WHERE source_kind = 'thread_entry'))
    UNION ALL
    SELECT 'question_answer', q.id, concat_ws(E'\n\n', q.question, q.answer)
      FROM workspace.questions q
     WHERE q.id = ANY(ARRAY(SELECT source_id FROM picked WHERE source_kind = 'question_answer'))
    UNION ALL
    SELECT v.kind, v.id, concat_ws(E'\n\n', t.user_prompt, v.unmet_ask, v.rationale)
      FROM (SELECT tv.*, p.source_kind AS kind
              FROM picked p JOIN memory.turn_verdicts tv ON tv.id = p.source_id
             WHERE p.source_kind IN ('unmet_ask', 'correction')) v
      JOIN memory.operator_turns t ON t.id = v.operator_turn_id
    UNION ALL
    SELECT 'learning', s.id,
           concat_ws(E'\n\n', array_to_string(k.topics, ', '), memory.session_learnings_text(k.learnings))
      FROM sessions.sessions s JOIN memory.session_knowledge k ON k.session_id = s.session_uuid
     WHERE s.id = ANY(ARRAY(SELECT source_id FROM picked WHERE source_kind = 'learning'))
    UNION ALL
    SELECT 'session', ov.id,
           concat_ws(E'\n\n', ov.overview, 'Topics: ' || NULLIF(array_to_string(ok.topics, ', '), ''))
      FROM sessions.sessions ov LEFT JOIN memory.session_knowledge ok ON ok.session_id = ov.session_uuid
     WHERE ov.id = ANY(ARRAY(SELECT source_id FROM picked WHERE source_kind = 'session'))
    UNION ALL
    SELECT 'mail', m.id, m.body
      FROM mailbox.messages m
     WHERE m.id = ANY(ARRAY(SELECT source_id FROM picked WHERE source_kind = 'mail'))
)
SELECT p.source_kind AS "source_kind!", p.source_id AS "source_id!", p.reference AS "reference!",
       p.source_updated_at, p.source_seq, p.source_hash, b.body AS "body!"
  FROM picked p
  JOIN bodies b ON b.source_kind = p.source_kind AND b.source_id = p.source_id
 ORDER BY p.project_rank, p.project_id, p.source_kind, p.source_id
