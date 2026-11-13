WITH latest AS (
    SELECT new_value
      FROM config.change_log
     WHERE subject_kind = 'build'
     ORDER BY id DESC
     LIMIT 1
), prev_settings AS (
    SELECT o.key, o.value
      FROM latest l, jsonb_each(l.new_value->'defaults'->'settings') o
), prev_toggles AS (
    SELECT o.key, o.value
      FROM latest l, jsonb_each(l.new_value->'defaults'->'toggles') o
), changed_keys AS (
    SELECT COALESCE(jsonb_agg(k.kind || ':' || k.key ORDER BY k.kind, k.key), '[]'::jsonb) AS keys,
           EXISTS (SELECT 1 FROM latest) AS has_previous
      FROM (
            SELECT 'setting' AS kind, COALESCE(n.key, o.key) AS key
              FROM jsonb_each($1::jsonb->'defaults'->'settings') n
              FULL JOIN prev_settings o USING (key)
             WHERE n.value IS DISTINCT FROM o.value
            UNION ALL
            SELECT 'toggle' AS kind, COALESCE(n.key, o.key) AS key
              FROM jsonb_each($1::jsonb->'defaults'->'toggles') n
              FULL JOIN prev_toggles o USING (key)
             WHERE n.value IS DISTINCT FROM o.value
      ) k
)
INSERT INTO config.change_log
    (subject_kind, subject_key, op, old_value, new_value, redacted, via,
     build_sha, application_name, detail)
SELECT 'build', 'mike', 'observe', lt.new_value, $1::jsonb, false, 'startup',
       $2, current_setting('application_name', true),
       CASE WHEN c.has_previous THEN jsonb_build_object('defaults_changed', c.keys)
            ELSE '{}'::jsonb END
  FROM changed_keys c
  LEFT JOIN latest lt ON true
 WHERE lt.new_value IS DISTINCT FROM $1::jsonb
RETURNING id AS "id!"
