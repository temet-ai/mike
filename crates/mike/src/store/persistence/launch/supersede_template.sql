UPDATE launch.templates
   SET superseded_by = $2
 WHERE id = $1
