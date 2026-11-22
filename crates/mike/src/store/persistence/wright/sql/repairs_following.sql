SELECT id, merge_sha, state
  FROM wright.repairs
 WHERE state IN ('landed', 'deployed')
 ORDER BY id
