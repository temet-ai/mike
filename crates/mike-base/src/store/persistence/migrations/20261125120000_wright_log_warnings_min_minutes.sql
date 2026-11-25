-- One short stall shared by several machines logged 30 slow-acquire warnings in 86 seconds and
-- counted as recurrence (Wright finding 36). A warning must now also appear in five separate
-- minutes of the window.
UPDATE wright.checks
   SET params = jsonb_set(params, '{min_minutes}', '5'),
       expects = 'No WARN log line of one target and shape repeats 30 times in at least 5 separate minutes within an hour',
       updated_at = now()
 WHERE name = 'log_warnings';
