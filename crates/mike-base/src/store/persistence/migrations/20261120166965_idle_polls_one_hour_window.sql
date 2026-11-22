-- idle_polls judges a statement over the last hour, not six. A six-hour base kept a
-- statement fixed early in the window flagged for most of a day, averaging its old
-- polling; a real poller runs at 60 or more an hour steadily and still shows within an hour.
UPDATE wright.checks
   SET params = params || '{"window_hours": 1}'::jsonb
 WHERE name = 'idle_polls';
