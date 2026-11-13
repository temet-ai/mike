-- Jev decisions (temet-ai/mike milestone 8, #212, #213, #220, #221).
--
-- llm.calls gains the role 'decision' for calls to OpenRouter's Decisions endpoint, the
-- outcome 'schema_error' for a reply that did not parse into typed answers, and
-- response_body, which keeps the typed answers (or the unparsed reply on a schema error) so
-- a replay can join answers to outcomes. llm.daily_spend's role CHECK is widened with
-- llm.calls', because its trigger copies NEW.role.
--
-- Every change is additive: a binary older than this migration keeps serving.

SET LOCAL lock_timeout = '10s';

ALTER TABLE llm.calls DROP CONSTRAINT IF EXISTS calls_role_check;
ALTER TABLE llm.calls
    ADD CONSTRAINT calls_role_check
        CHECK (role IN ('steerer', 'session_judge', 'judge_worker', 'lesson_relations',
                        'eval', 'lesson_proposer', 'decision'));

ALTER TABLE llm.daily_spend DROP CONSTRAINT IF EXISTS daily_spend_role_check;
ALTER TABLE llm.daily_spend
    ADD CONSTRAINT daily_spend_role_check
        CHECK (role IN ('steerer', 'session_judge', 'judge_worker', 'lesson_relations',
                        'eval', 'lesson_proposer', 'decision'));

ALTER TABLE llm.calls DROP CONSTRAINT IF EXISTS calls_outcome_check;
ALTER TABLE llm.calls
    ADD CONSTRAINT calls_outcome_check
        CHECK (outcome IN ('ok', 'http_error', 'timeout', 'truncated', 'transport_error',
                           'refused_ceiling', 'refused_rate', 'refused_spend', 'schema_error'));

ALTER TABLE llm.calls ADD COLUMN IF NOT EXISTS response_body jsonb;

COMMENT ON COLUMN llm.calls.response_body IS
    'What the call answered, for calls that keep it (role decision: the typed answers, or the unparsed reply on schema_error).';
