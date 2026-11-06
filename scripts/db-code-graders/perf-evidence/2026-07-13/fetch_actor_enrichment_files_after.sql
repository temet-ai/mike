SELECT DISTINCT tu.file_path
  FROM tools.uses tu
  JOIN requests.requests r ON r.id = tu.request_id
 WHERE r.session_id = 481321 AND tu.file_path IS NOT NULL
 ORDER BY tu.file_path;
