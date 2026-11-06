INSERT INTO memory.lesson_proposer_daily_budget (day, consumed)
VALUES ((NOW() AT TIME ZONE 'UTC')::date, 1)
ON CONFLICT (day) DO UPDATE
   SET consumed = memory.lesson_proposer_daily_budget.consumed + 1
 WHERE memory.lesson_proposer_daily_budget.consumed < 400
RETURNING consumed
