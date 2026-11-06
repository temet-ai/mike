UPDATE agents.work_items
   SET state = 'killed',
       finished_at = '2026-10-01T19:48:45Z',
       exit_code = NULL,
       summary = COALESCE(summary, 'read from the last line of the job''s output file')
 WHERE id = 12371
   AND state = 'running'
