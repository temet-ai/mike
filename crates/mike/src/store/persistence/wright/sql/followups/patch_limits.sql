UPDATE wright.followup_limits
   SET repo = COALESCE($1::text, repo),
       daily_filed = COALESCE($2::int, daily_filed),
       daily_handoffs = COALESCE($3::int, daily_handoffs),
       max_attempts = COALESCE($4::int, max_attempts),
       recurrence_actions = COALESCE($5::int, recurrence_actions),
       recurrence_days = COALESCE($6::int, recurrence_days),
       fixers = COALESCE($7::jsonb, fixers),
       fix_host = COALESCE($8::text, fix_host),
       updated_at = now()
 WHERE id
RETURNING repo, daily_filed, daily_handoffs, max_attempts, recurrence_actions, recurrence_days,
          fixers, fix_host, updated_at
