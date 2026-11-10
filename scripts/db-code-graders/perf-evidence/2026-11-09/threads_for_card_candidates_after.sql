SELECT t.id AS thread_id, t.project_id AS project_id,
       concat_ws(E'\n', NULLIF(btrim(t.title), ''), NULLIF(btrim(t.goal), ''),
                 NULLIF(btrim(s.tldr), ''), NULLIF(btrim(s.where_we_are), ''),
                 NULLIF(btrim(s.next_up), '')) AS query_text
  FROM work.threads t
  LEFT JOIN work.thread_state s ON s.thread_id = t.id
 WHERE t.status IS NULL
    OR t.status NOT IN ('done', 'complete', 'parked', 'superseded')
 ORDER BY t.id;
