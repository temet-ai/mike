ALTER TABLE memory.lesson_proposer_daily_budget
    ADD COLUMN IF NOT EXISTS maintenance_consumed bigint NOT NULL DEFAULT 0
        CHECK (maintenance_consumed >= 0);

COMMENT ON COLUMN memory.lesson_proposer_daily_budget.maintenance_consumed IS
    'Model calls the lesson maintenance passes (the shape rewrite and the placement pass) reserved '
    'on this UTC day. Counted apart from consumed, the correction proposer''s own counter, so '
    'maintenance never draws on the judge_lessons.max_per_day allowance for new corrections; its '
    'ceiling is judge_lessons.maintenance_max_per_day.';

CREATE TABLE memory.lesson_maintenance_refusals (
    pass       text PRIMARY KEY CHECK (pass IN ('shape', 'placement')),
    reason     text NOT NULL CHECK (reason IN ('daily_budget', 'provider_limit')),
    cap        bigint NOT NULL CHECK (cap >= 0),
    refused_at timestamptz NOT NULL DEFAULT now(),
    logged_at  timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE memory.lesson_maintenance_refusals IS
    'The latest time each lesson maintenance pass (shape or placement) was refused a model call, '
    'and why: daily_budget (the maintenance counter reached its cap) or provider_limit (the call '
    'limiter refused it). One row per pass, overwritten on every refusal. refused_at feeds the '
    'llm_role_health capability row; logged_at limits the matching info log line to once an hour.';
