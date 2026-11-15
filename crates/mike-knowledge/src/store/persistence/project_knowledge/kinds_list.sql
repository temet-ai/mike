SELECT k.kind AS "kind!", k.label AS "label!", k.description, k.body_view AS "body_view!",
       k.settle_secs AS "settle_secs!", k.enabled AS "enabled!", k.built_in AS "built_in!",
       k.generated_view AS "generated_view!", k.source_table, k.key_column, k.title_column, k.body_columns,
       k.project_column, k.created_at AS "created_at!",
       s.items AS "items!", s.embedded AS "embedded!", s.waiting AS "waiting!", s.failed AS "failed!",
       s.newest_error, s.newest_error_at, s.oldest_waiting_ready_at,
       COALESCE(s.oldest_waiting_ready_at < now() - interval '5 minutes', false) AS "behind!",
       COALESCE(w.watches, '[]'::jsonb) AS "watches!"
  FROM memory.knowledge_kinds k
  JOIN memory.knowledge_kind_stats() s ON s.kind = k.kind
  LEFT JOIN LATERAL (
      SELECT jsonb_agg(jsonb_build_object(
                 'id', kw.id, 'table', kw.watched_table, 'key_column', kw.key_column,
                 'update_columns', kw.update_columns, 'via_table', kw.via_table,
                 'via_match_column', kw.via_match_column, 'via_key_column', kw.via_key_column)
             ORDER BY kw.id) AS watches
        FROM memory.knowledge_kind_watches kw
       WHERE kw.kind = k.kind) w ON true
 ORDER BY k.built_in DESC, k.kind
