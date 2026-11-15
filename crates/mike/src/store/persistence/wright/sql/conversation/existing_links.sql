SELECT 'problem' AS kind, id FROM wright.problems WHERE id = ANY($1)
UNION ALL
SELECT 'episode' AS kind, id FROM wright.episodes WHERE id = ANY($2)
UNION ALL
SELECT 'action' AS kind, id FROM wright.actions WHERE id = ANY($3)
UNION ALL
SELECT 'finding' AS kind, id FROM wright.findings WHERE id = ANY($4)
UNION ALL
SELECT 'repair' AS kind, id FROM wright.repairs WHERE id = ANY($5)
