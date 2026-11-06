SELECT DISTINCT tu.file_path
  FROM tools.uses tu
 WHERE tu.session_id = 100749 AND tu.file_path IS NOT NULL
 ORDER BY tu.file_path;
