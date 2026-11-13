-- llm.daily_spend — a per-day rollup of llm.calls, kept exact by a trigger.
--
-- crates/mike/src/store/persistence/llm_calls.rs aggregates llm.calls
-- straight off the raw table on every /api/llm/spend and
-- /api/usage/openrouter read: `llm_spend_by_day_caller` alone was measured
-- at 45ms for a 30-day window over ~65k rows, doing a SeqScan that grows
-- linearly with table size — a 90-day window already blows the 50ms read
-- budget in .claude/rules/db-performance-evaluation.md. `sum_llm_spend_range`
-- and the by_day/by_model/by_role group-bys (`llm_spend_grouped`) have the
-- same SeqScan shape.
--
-- This table pre-aggregates llm.calls to one row per
-- (day, role, steerer_id, model), maintained by an AFTER INSERT trigger on
-- llm.calls so every read query becomes a scan of a table with, at most, a
-- few dozen rows per day instead of the full history.
--
-- Grain: `day` is the UTC calendar day of `created_at`
-- ((created_at AT TIME ZONE 'utc')::date), matching the existing
-- `to_char(date_trunc('day', created_at AT TIME ZONE 'utc'), ...)` grouping
-- already used by llm_spend_by_day / llm_spend_by_day_caller. `steerer_id`
-- is carried through unaggregated across steerers (matches the existing
-- by_day_caller grain); `model` is present so llm_spend_by_model can read
-- straight off this table too, and is summed away by the day/role/steerer
-- readers.
--
-- steerer_id has NO foreign key, for the same reason llm.calls.steerer_id
-- has none (20260829000002_llm_calls.sql): a call can be recorded, and this
-- rollup must still update, whether or not the steerer row it names exists
-- right now. The rollup is a derived aggregate of llm.calls, so it inherits
-- that table's identity rules rather than inventing its own.
--
-- Trigger scope: AFTER INSERT only. llm.calls is written exclusively by
-- insert_llm_call (crates/mike/src/store/persistence/llm_calls.rs) — grepping
-- crates/mike/src for `UPDATE llm.calls` finds no writer, and
-- insert_llm_call itself never issues one. If an UPDATE path is ever added
-- (e.g. a cost correction landing after the row is written), extend this
-- trigger with a matching AFTER UPDATE handler that applies NEW-OLD deltas
-- to the same conflict target; until then an UPDATE trigger would be dead
-- code guarding against a write path that does not exist.
--
-- The unique index uses NULLS NOT DISTINCT (Postgres 15+; this DB runs
-- 17.11) so ON CONFLICT can target rows with steerer_id IS NULL — the
-- common case for session_judge/judge_worker/lesson_relations/eval calls,
-- which never carry a steerer.

CREATE TABLE IF NOT EXISTS llm.daily_spend (
    id                 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    day                date NOT NULL,
    role               text NOT NULL
                         CHECK (role IN ('steerer', 'session_judge', 'judge_worker',
                                         'lesson_relations', 'eval')),
    steerer_id         bigint,
    model              text NOT NULL,
    call_count         bigint NOT NULL DEFAULT 0,
    cost_usd           double precision NOT NULL DEFAULT 0,
    prompt_tokens      bigint NOT NULL DEFAULT 0,
    completion_tokens  bigint NOT NULL DEFAULT 0,
    reasoning_tokens   bigint NOT NULL DEFAULT 0,
    failed_count       bigint NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_daily_spend_natural_key
    ON llm.daily_spend (day, role, steerer_id, model) NULLS NOT DISTINCT;

COMMENT ON TABLE llm.daily_spend IS
    'Per-(day, role, steerer_id, model) rollup of llm.calls, kept exact by '
    'trg_llm_calls_daily_spend. Read by every /api/llm/spend and '
    '/api/usage/openrouter query instead of scanning llm.calls directly.';

CREATE OR REPLACE FUNCTION llm.apply_call_to_daily_spend()
RETURNS trigger AS $$
BEGIN
    INSERT INTO llm.daily_spend AS ds
        (day, role, steerer_id, model, call_count, cost_usd,
         prompt_tokens, completion_tokens, reasoning_tokens, failed_count)
    VALUES (
        (NEW.created_at AT TIME ZONE 'utc')::date,
        NEW.role,
        NEW.steerer_id,
        NEW.model,
        1,
        COALESCE(NEW.cost_usd, 0.0),
        COALESCE(NEW.prompt_tokens, 0),
        COALESCE(NEW.completion_tokens, 0),
        COALESCE(NEW.reasoning_tokens, 0),
        CASE WHEN NEW.outcome NOT IN ('ok', 'truncated') THEN 1 ELSE 0 END
    )
    ON CONFLICT (day, role, steerer_id, model) DO UPDATE SET
        call_count = ds.call_count + 1,
        cost_usd = ds.cost_usd + COALESCE(NEW.cost_usd, 0.0),
        prompt_tokens = ds.prompt_tokens + COALESCE(NEW.prompt_tokens, 0),
        completion_tokens = ds.completion_tokens + COALESCE(NEW.completion_tokens, 0),
        reasoning_tokens = ds.reasoning_tokens + COALESCE(NEW.reasoning_tokens, 0),
        failed_count = ds.failed_count
            + CASE WHEN NEW.outcome NOT IN ('ok', 'truncated') THEN 1 ELSE 0 END;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_llm_calls_daily_spend ON llm.calls;

CREATE TRIGGER trg_llm_calls_daily_spend
    AFTER INSERT ON llm.calls
    FOR EACH ROW EXECUTE FUNCTION llm.apply_call_to_daily_spend();

-- One-shot, set-based backfill of every pre-existing llm.calls row. Runs
-- once (the table is created a few lines above, in this same migration), so
-- ON CONFLICT DO NOTHING is a defensive no-op rather than the normal path.
INSERT INTO llm.daily_spend
    (day, role, steerer_id, model, call_count, cost_usd,
     prompt_tokens, completion_tokens, reasoning_tokens, failed_count)
SELECT (created_at AT TIME ZONE 'utc')::date AS day,
       role,
       steerer_id,
       model,
       COUNT(*)::bigint AS call_count,
       COALESCE(SUM(cost_usd), 0.0)::double precision AS cost_usd,
       COALESCE(SUM(prompt_tokens), 0)::bigint AS prompt_tokens,
       COALESCE(SUM(completion_tokens), 0)::bigint AS completion_tokens,
       COALESCE(SUM(reasoning_tokens), 0)::bigint AS reasoning_tokens,
       COUNT(*) FILTER (WHERE outcome NOT IN ('ok', 'truncated'))::bigint AS failed_count
  FROM llm.calls
 GROUP BY 1, 2, 3, 4
ON CONFLICT (day, role, steerer_id, model) DO NOTHING;
