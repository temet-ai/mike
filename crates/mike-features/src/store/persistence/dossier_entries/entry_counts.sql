SELECT kind, data->>'class' AS "class?", data->>'polarity' AS "polarity?",
       status, (data->>'registry_coverage')::boolean AS "registry_coverage?", count(*) AS "entries!"
  FROM memory.dossier_entries
 WHERE effective_at >= $1
 GROUP BY 1, 2, 3, 4, 5
 ORDER BY 1, 2, 3, 4, 5
