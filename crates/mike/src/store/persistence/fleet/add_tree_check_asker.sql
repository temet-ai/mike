UPDATE launch.tree_checks
   SET askers = CASE WHEN $2 = ANY (askers) THEN askers ELSE array_append(askers, $2) END
 WHERE id = $1
