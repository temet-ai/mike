-- grader-allow: single-row UPDATE by primary key, written once when the receipt's process starts
UPDATE launch.tree_checks
SET executing_at = now()
WHERE id = $1 AND executing_at IS NULL
