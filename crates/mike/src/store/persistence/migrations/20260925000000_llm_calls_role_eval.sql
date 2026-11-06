--
-- llm.calls role: add 'eval'.
--
-- The role CHECK was introduced in 20260829000002_llm_calls.sql as a closed
-- set mirroring crates/mike/src/llm/usage.rs LlmRole, with the note that a
-- fifth caller "extends this CHECK in the migration that adds it". The
-- fifth caller is the evaluation role: the offline `mike eval run` gate and
-- the shadow judge sweep. They need a role of their own for two reasons.
-- First, a call with no role is dropped by the usage sink, not recorded
-- (config/persist_bootstrap.rs, the `as_db_value()` None branch), so eval
-- spend would be invisible. Second, borrowing judge_worker would spend the
-- production judge's guaranteed share and hide eval cost inside production
-- cost, the same misattribution that sits latent in the correction
-- classifier's construction (config/app_state/construction_llm.rs:33), which
-- bills to steerer. That code is not wired on the live binary and made no
-- call in the P0 window (plans/judgement-context/p0/RESULTS.md finding 4);
-- JC12 retires it.
--
-- The constraint is dropped and re-added whole, the same idiom as the
-- feature_toggles key CHECK migrations. Validation scans llm.calls once
-- (54,176 rows, 184 MB including TOAST, on 2026-09-23); every existing row
-- already satisfies the wider set.

ALTER TABLE llm.calls
    DROP CONSTRAINT IF EXISTS calls_role_check;

ALTER TABLE llm.calls
    ADD CONSTRAINT calls_role_check
    CHECK (role IN ('steerer', 'session_judge', 'judge_worker', 'lesson_relations', 'eval'));

COMMENT ON TABLE llm.calls IS
    'One row per outbound OpenRouter call across every LLM caller (steerer, '
    'session_judge, judge_worker, lesson_relations, eval). Written when the '
    'llm.record_usage setting is on; a write failure here must never fail '
    'the call it is recording.';
