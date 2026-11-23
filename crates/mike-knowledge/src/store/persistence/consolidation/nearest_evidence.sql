WITH nearest AS MATERIALIZED (
    SELECT e.reference, e.source_kind, e.source_id, e.item_id, c.body,
           c.embedding <=> $2::text::public.vector AS distance
      FROM memory.knowledge_embedding_chunks c
      JOIN memory.knowledge_embedding_state e ON e.id = c.state_id
     WHERE e.project_id = $1 AND e.completed_at IS NOT NULL AND e.reference IS NOT NULL
       AND e.source_kind IN ('correction', 'unmet_ask', 'learning', 'question_answer', 'note',
                             'thread_entry', 'mail', 'thread', 'session')
     ORDER BY c.embedding <=> $2::text::public.vector
     LIMIT $3
)
SELECT n.reference AS "reference!", n.source_kind AS "kind!", n.body AS "body!",
       (1 - n.distance)::float8 AS "similarity!",
       COALESCE(tv.created_at, sk.generated_at, q.settled_at, q.created_at, sn.created_at, tl.created_at,
                COALESCE(m.sent_at, m.queued_at), th.last_message_at,
                COALESCE(ss.overview_generated_at, ss.last_seen), i.changed_at) AS "dated_at!"
  FROM nearest n
  JOIN memory.knowledge_items i ON i.id = n.item_id
  LEFT JOIN memory.turn_verdicts tv ON n.source_kind IN ('correction', 'unmet_ask') AND tv.id = n.source_id
  LEFT JOIN sessions.sessions ss ON n.source_kind IN ('learning', 'session') AND ss.id = n.source_id
  LEFT JOIN memory.session_knowledge sk ON n.source_kind = 'learning' AND sk.session_id = ss.session_uuid
  LEFT JOIN workspace.questions q ON n.source_kind = 'question_answer' AND q.id = n.source_id
  LEFT JOIN workspace.session_notes sn ON n.source_kind = 'note' AND sn.id = n.source_id
  LEFT JOIN work.thread_log tl ON n.source_kind = 'thread_entry' AND tl.id = n.source_id
  LEFT JOIN mailbox.messages m ON n.source_kind = 'mail' AND m.id = n.source_id
  LEFT JOIN mailbox.threads th ON n.source_kind = 'thread' AND th.id = n.source_id
 ORDER BY n.distance, n.reference
