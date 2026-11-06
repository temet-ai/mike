UPDATE launch.launches
   SET sibling_wait_seconds = 94,
       waited_for_seat      = 'rust-opus-T4'
 WHERE id = (SELECT l.id
               FROM launch.launches l
              WHERE l.session_uuid = '9dda3b31-f7e6-4547-9e13-33b566cfab97'
              ORDER BY l.created_at DESC, l.id DESC
              LIMIT 1)
RETURNING id
