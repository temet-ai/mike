INSERT INTO workspace.change_events
    (project_id, checkout, at, kind, session_id, run_id, task, paths, subject, detail, before_sha, after_sha)
SELECT i.project_id, i.checkout, i.at, i.kind, i.session_id, i.run_id, i.task,
       string_to_array(i.paths, chr(31)), i.subject, i.detail, i.before_sha, i.after_sha
  FROM unnest($1::bigint[], $2::text[], $3::timestamptz[], $4::text[], $5::bigint[],
              $6::text[], $7::text[], $8::text[], $9::text[], $10::jsonb[],
              $11::text[], $12::text[])
       WITH ORDINALITY AS i(project_id, checkout, at, kind, session_id, run_id, task,
                            paths, subject, detail, before_sha, after_sha, ord)
 ORDER BY i.ord
RETURNING id
