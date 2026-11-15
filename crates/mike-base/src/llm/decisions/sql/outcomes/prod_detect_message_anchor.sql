WITH judged AS (
    SELECT r.id AS record_id,
           btrim(r.state ->> 'message', E' \t\r\n') AS message,
           (r.state ->> 'message_chars')::int AS message_chars,
           (r.answers -> 'only_nudge' ->> 'noul')::float8 AS noul
      FROM decisions.records r
      LEFT JOIN decisions.outcomes done
        ON done.record_id = r.id AND done.label_source = 'message_anchor'
     WHERE r.point = 'prod_detect'
       AND r.created_at >= now() - interval '30 days'
       AND done.record_id IS NULL
       AND r.state ->> 'message' IS NOT NULL
       AND r.answers -> 'only_nudge' ->> 'noul' IS NOT NULL
), truth AS (
    SELECT j.*,
           CASE WHEN j.message ~* '^(continue|go on|go ahead|carry on|proceed|keep going|\?+|status\??|any update\??|ping|still there\??)[.!?]*$' THEN true
                WHEN j.message_chars >= 60 THEN false
           END AS is_nudge
      FROM judged j
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT t.record_id,
       CASE WHEN (t.noul >= 0.5) = t.is_nudge THEN 'right' ELSE 'wrong' END,
       'message_anchor',
       jsonb_build_object('message_chars', t.message_chars,
                          'only_nudge', t.noul,
                          'truth_is_nudge', t.is_nudge)
  FROM truth t
 WHERE t.is_nudge IS NOT NULL
ON CONFLICT (record_id, label_source, label) DO NOTHING
