UPDATE launch.launches
   SET sibling_wait_seconds = $2,
       waited_for_seat      = $3
 WHERE id = (SELECT l.id
               FROM launch.launches l
              WHERE l.session_uuid = $1
              ORDER BY l.created_at DESC, l.id DESC
              LIMIT 1)
RETURNING id
