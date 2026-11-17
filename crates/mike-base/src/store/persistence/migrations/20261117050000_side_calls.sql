-- Side calls (work thread side-call-bookkeeping): mike fills session bookkeeping with small
-- calls on a cheap model, Gemma on a local server first.
--
-- llm.calls and llm.daily_spend gain the role 'side_call', so those calls are attributed and
-- budgeted apart from the steerer and judges.
--
-- sessions.session_goals gains goal_source: who last wrote goal. A side call writes goal only
-- while it is empty or was itself written by a side call, so a goal a session declared is never
-- replaced. Existing rows keep NULL, which counts as the session's own.
--
-- Every change is additive: a binary older than this migration keeps serving.

SET LOCAL lock_timeout = '10s';

ALTER TABLE llm.calls DROP CONSTRAINT IF EXISTS calls_role_check;
ALTER TABLE llm.calls
    ADD CONSTRAINT calls_role_check
        CHECK (role IN ('steerer', 'session_judge', 'judge_worker', 'lesson_relations',
                        'eval', 'lesson_proposer', 'decision', 'side_call'));

ALTER TABLE llm.daily_spend DROP CONSTRAINT IF EXISTS daily_spend_role_check;
ALTER TABLE llm.daily_spend
    ADD CONSTRAINT daily_spend_role_check
        CHECK (role IN ('steerer', 'session_judge', 'judge_worker', 'lesson_relations',
                        'eval', 'lesson_proposer', 'decision', 'side_call'));

ALTER TABLE sessions.session_goals ADD COLUMN IF NOT EXISTS goal_source text;

COMMENT ON COLUMN sessions.session_goals.goal_source IS
    'Who last wrote goal (a registration path, mcp, rest, or side-call). NULL on rows written before 2026-11-17 counts as the session''s own.';
