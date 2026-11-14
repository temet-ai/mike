SELECT 'problem' AS kind, id FROM wright.problems WHERE id = ANY($1)
UNION ALL
SELECT 'episode' AS kind, id FROM wright.episodes WHERE id = ANY($2)
