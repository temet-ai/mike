UPDATE launch.conditions
   SET outcome = $2
 WHERE id = $1 AND state IN ('fired', 'missed')
