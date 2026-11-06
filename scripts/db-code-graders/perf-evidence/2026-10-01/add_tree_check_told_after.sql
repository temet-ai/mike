UPDATE launch.tree_checks SET told = array_append(told, 'db-dev-T2') WHERE id = 7 AND NOT ('db-dev-T2' = ANY (told))
