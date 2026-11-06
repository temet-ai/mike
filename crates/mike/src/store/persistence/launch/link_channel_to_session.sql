UPDATE remote_control.channels
   SET local_session_uuid = $2,
       local_cwd          = COALESCE(NULLIF(local_cwd, ''), $3)
 WHERE cse_id = $1
   AND local_session_uuid IS NULL
RETURNING id
