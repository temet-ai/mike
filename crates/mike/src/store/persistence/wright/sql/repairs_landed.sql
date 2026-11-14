SELECT id, merge_sha
  FROM wright.repairs
 WHERE state = 'landed' AND merge_sha IS NOT NULL
 ORDER BY id
