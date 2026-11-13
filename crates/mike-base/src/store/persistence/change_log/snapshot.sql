WITH stored AS (
    SELECT DISTINCT ON (subject_kind, subject_key)
           subject_kind, subject_key, new_value, redacted, changed_at, id, via
      FROM config.change_log
     WHERE changed_at <= $1
       AND subject_kind IN ('setting', 'toggle', 'steerer')
       AND subject_kind = ANY($2::text[])
     ORDER BY subject_kind, subject_key, id DESC
), build_at AS (
    SELECT id, changed_at, via, new_value
      FROM config.change_log
     WHERE subject_kind = 'build' AND changed_at <= $1
     ORDER BY id DESC
     LIMIT 1
), defaults AS (
    SELECT 'setting'::text AS subject_kind, d.key AS subject_key, d.value
      FROM build_at b, jsonb_each(b.new_value->'defaults'->'settings') d
    UNION ALL
    SELECT 'toggle'::text, d.key, d.value
      FROM build_at b, jsonb_each(b.new_value->'defaults'->'toggles') d
), joined AS (
    SELECT subject_kind, subject_key,
           s.new_value AS stored_value, s.redacted AS stored_redacted,
           s.changed_at AS stored_at, s.id AS stored_id, s.via AS stored_via,
           (s.subject_key IS NOT NULL) AS has_row,
           d.value AS default_value
      FROM stored s
      FULL JOIN defaults d USING (subject_kind, subject_key)
)
SELECT j.subject_kind AS "subject_kind!", j.subject_key AS "subject_key!",
       CASE WHEN j.stored_value IS NOT NULL OR j.subject_kind = 'steerer' THEN j.stored_value
            ELSE j.default_value END AS "value?",
       CASE WHEN j.stored_value IS NOT NULL THEN 'stored'
            WHEN j.subject_kind = 'steerer' AND j.has_row THEN 'stored'
            WHEN j.default_value IS NOT NULL THEN 'code_default'
            ELSE 'unknown' END AS "source!",
       COALESCE(j.stored_redacted AND j.stored_value IS NOT NULL, false) AS "redacted!",
       CASE WHEN j.stored_value IS NULL AND j.default_value IS NOT NULL
                 AND j.subject_kind <> 'steerer' THEN b.changed_at
            ELSE j.stored_at END AS "changed_at?",
       CASE WHEN j.stored_value IS NULL AND j.default_value IS NOT NULL
                 AND j.subject_kind <> 'steerer' THEN b.id
            ELSE j.stored_id END AS "change_id?",
       CASE WHEN j.stored_value IS NULL AND j.default_value IS NOT NULL
                 AND j.subject_kind <> 'steerer' THEN b.via
            ELSE j.stored_via END AS "via?"
  FROM joined j
  LEFT JOIN build_at b ON true
 WHERE j.subject_kind = ANY($2::text[])
 ORDER BY j.subject_kind, j.subject_key
