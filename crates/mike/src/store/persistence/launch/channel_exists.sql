SELECT EXISTS (
    SELECT 1
      FROM remote_control.channels
     WHERE cse_id = $1
) AS "present!"
