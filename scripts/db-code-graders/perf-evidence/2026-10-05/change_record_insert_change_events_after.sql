INSERT INTO workspace.change_events
    (project_id, checkout, at, kind, session_id, run_id, task, paths, subject, detail, before_sha, after_sha)
SELECT i.project_id, i.checkout, i.at, i.kind, i.session_id, i.run_id, i.task,
       string_to_array(i.paths, chr(31)), i.subject, i.detail, i.before_sha, i.after_sha
  FROM unnest(ARRAY[(SELECT min(id) FROM projects.projects), (SELECT min(id) FROM projects.projects)]::bigint[],
              ARRAY['/work/checkout-3','/work/checkout-3']::text[], ARRAY[now(), now()]::timestamptz[],
              ARRAY['edit','note']::text[], ARRAY[1348628, NULL]::bigint[], ARRAY[NULL, NULL]::text[],
              ARRAY[NULL, NULL]::text[], ARRAY['a.rs' || chr(31) || 'b.rs', '']::text[], ARRAY['s1','s2']::text[],
              ARRAY['{}','{}']::jsonb[], ARRAY[NULL, NULL]::text[], ARRAY[NULL, NULL]::text[])
       WITH ORDINALITY AS i(project_id, checkout, at, kind, session_id, run_id, task,
                            paths, subject, detail, before_sha, after_sha, ord)
 ORDER BY i.ord
RETURNING id
