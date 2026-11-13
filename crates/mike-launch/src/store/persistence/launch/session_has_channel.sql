SELECT EXISTS (
    SELECT 1
      FROM remote_control.channels
     WHERE local_session_uuid = $1
) AS "present!"
