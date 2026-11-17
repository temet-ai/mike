SELECT source, max(at) AS newest
  FROM wright.log_lines
 WHERE machine_uuid = $1
 GROUP BY source
