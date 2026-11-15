SELECT format('%I.%I', n.nspname, c.relname) AS "name!",
       jsonb_agg(jsonb_build_object('name', a.attname, 'type', format_type(a.atttypid, a.atttypmod))
                 ORDER BY a.attnum) AS "columns!: sqlx::types::Json<Vec<TableColumn>>"
  FROM pg_class c
  JOIN pg_namespace n ON n.oid = c.relnamespace
  JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
 WHERE c.relkind IN ('r', 'p', 'v')
   AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
   AND n.nspname NOT LIKE 'pg\_temp\_%'
   AND n.nspname NOT LIKE 'pg\_toast\_temp\_%'
   AND NOT (c.relkind IN ('r', 'p') AND n.nspname = 'memory' AND c.relname LIKE 'knowledge\_%')
 GROUP BY n.nspname, c.relname
 ORDER BY n.nspname, c.relname
