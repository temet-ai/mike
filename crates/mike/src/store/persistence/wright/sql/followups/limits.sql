SELECT repo, daily_filed, daily_handoffs, max_attempts, recurrence_actions, recurrence_days,
       fixers, fix_host, updated_at
  FROM wright.followup_limits
 WHERE id
