UPDATE wright.engineer_limits
   SET daily_starts = COALESCE($1, daily_starts),
       wall_seconds = COALESCE($2, wall_seconds),
       tool_calls = COALESCE($3, tool_calls),
       tokens = COALESCE($4, tokens),
       updated_at = now()
 WHERE id
RETURNING daily_starts, wall_seconds, tool_calls, tokens, updated_at
