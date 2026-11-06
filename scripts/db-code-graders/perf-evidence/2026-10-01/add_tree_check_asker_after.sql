UPDATE launch.tree_checks
   SET askers = CASE WHEN 'db-dev-T2' = ANY (askers) THEN askers ELSE array_append(askers, 'db-dev-T2') END
 WHERE id = 7
