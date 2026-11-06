WITH cur AS (
    SELECT DISTINCT ON (role, subject_key)
           id, role, subject_key, components, cause, created_at, status, gate_run_id,
           override_reason
      FROM eval.bundles
     WHERE origin = 'resolver' AND superseded_at IS NULL
     ORDER BY role, subject_key, created_at DESC, id DESC
)
SELECT 'bundle:' || cur.id::text AS "subject!",
       jsonb_build_object('role', cur.role, 'subject_key', cur.subject_key,
                          'components', cur.components, 'cause', cur.cause,
                          'created_at', cur.created_at, 'previous_id', prev.id,
                          'previous_components', prev.components) AS "detail!"
  FROM cur
  LEFT JOIN eval.bundles prev ON prev.superseded_by_id = cur.id
 WHERE cur.role = ANY($1::text[])
   AND cur.status = 'live_ungated' AND cur.gate_run_id IS NULL AND cur.override_reason IS NULL
   AND cur.created_at < now() - make_interval(hours => $2::int)
   AND NOT EXISTS (SELECT 1 FROM config.settings ls
                    WHERE ls.key = cur.role || '.live_bundle_id'
                      AND jsonb_typeof(ls.value) = 'number' AND (ls.value)::text::numeric > 0)
   AND (cur.role <> 'steerer'
        OR EXISTS (SELECT 1 FROM steering.steerers st
                    WHERE st.id::text = cur.subject_key AND st.enabled))
 ORDER BY cur.id
