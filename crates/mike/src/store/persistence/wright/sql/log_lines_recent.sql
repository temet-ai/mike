SELECT machine_uuid, source, at, level, target, message, raw
  FROM wright.log_lines
 WHERE at >= $1
   AND level = $2
 ORDER BY at, id
