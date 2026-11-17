DELETE FROM wright.log_lines
 WHERE machine_uuid = $1
   AND at < $2
