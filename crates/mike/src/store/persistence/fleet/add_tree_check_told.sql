UPDATE launch.tree_checks
   SET told = array_append(told, $2)
 WHERE id = $1
   AND NOT ($2 = ANY (told))
