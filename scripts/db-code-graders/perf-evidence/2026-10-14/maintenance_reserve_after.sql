INSERT INTO memory.lesson_proposer_daily_budget (day, consumed, maintenance_consumed)
VALUES ((NOW() AT TIME ZONE 'UTC')::date, 0, 1)
ON CONFLICT (day) DO UPDATE
   SET maintenance_consumed = memory.lesson_proposer_daily_budget.maintenance_consumed + 1
 WHERE memory.lesson_proposer_daily_budget.maintenance_consumed < 400
RETURNING maintenance_consumed
