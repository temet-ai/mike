UPDATE launch.launches
   SET opened           = true,
       opened_at        = COALESCE(opened_at, now()),
       workspace_ref    = $2,
       tab_ref          = $3,
       surface_ref      = $4,
       pid              = $5,
       actual_placement = $6
 WHERE id = $1
RETURNING id
