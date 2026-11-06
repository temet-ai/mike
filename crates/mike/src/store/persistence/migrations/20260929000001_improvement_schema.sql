-- Judgement-at-work (JC21): the store for the improvement loop. A corrected
-- live verdict becomes a lesson candidate; an approved candidate or directive
-- proposal becomes an injected lesson; every action (lesson, directive, guard
-- question) gets a measured before/after or holdback/treated corrected share.
-- See artifacts/2026-09-29/judgement-at-work/PLAN.md section 2.
--
-- Two new lesson_sources rows: 'directive' (id 5, JC18 J6 approvals) and
-- 'judge' (id 6, this proposer). The table already holds 1 agent, 2 operator,
-- 3 copilot, 4 harvester (20260613000001_memory_recall_schema.sql); 5 and 6
-- are the next free ids, matching the worklog's package row.
--
-- One new lesson_archived_reasons row, 'rejected_by_operator' (id 6): a
-- candidate the operator rejected in the SPA. Distinct from the existing
-- 'operator' reason (id 5, used by delete_lesson for an outright delete of
-- any lesson) — this one is specific to the improvement review flow and
-- carries the operator's structured reason (not_a_correction, too_specific,
-- wrong, other) in the config.change_log row the approve/reject write also
-- makes, not in this vocabulary table.
--
-- feature_toggles gains 'judge_lessons' (JC21 lesson proposer). It sends
-- captured turn text to an LLM (the proposer call), so
-- default-off-mutation-and-egress.md requires the code default off; this
-- migration inserts (false, false) rather than the "on for this install"
-- value PLAN.md section 2.6 records — that flip is a deploy-time operator
-- action through /settings, logged by the existing feature_toggles
-- change_log trigger, not something this migration should hardcode.
--
-- eval.rates.family gains 'action_effect' (JC21 measurement, PLAN 2.4): one
-- row per (action, population) where population is before/after for a lesson
-- or directive and holdback/treated for a guard question, metric share,
-- class corrected. Both the bare family CHECK and the family+metric pairing
-- CHECK (widened by 20260925000801_change_log_calibration.sql) need it.
--
-- config.change_log.subject_kind gains 'lesson_decision' (an approve/reject
-- of a lesson candidate). 'directive_decision' already exists
-- (20260925000801); BUILD-TIME STEP per that migration's own note: this list
-- is regenerated from main's current constraint, so re-derive it fresh
-- rather than trust an older draft.
--
-- Expand-only.

INSERT INTO memory.lesson_sources (id, code) OVERRIDING SYSTEM VALUE VALUES
    (5, 'directive'),
    (6, 'judge')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.lesson_sources_id_seq', 6);

INSERT INTO memory.lesson_archived_reasons (id, code) OVERRIDING SYSTEM VALUE VALUES
    (6, 'rejected_by_operator')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.lesson_archived_reasons_id_seq', 6);

-- Derived from origin/main's newest widening, 20260928120100_launch_brief_toggle.sql
-- (26 keys ending 'launch_brief'), not the older 20260928100200 copy this
-- migration originally rewrote from — an outside commit (221d9ac2) landed
-- 20260928120100 on origin/main after this migration was first drafted, and
-- a migration that rewrites a CHECK must always carry forward every key the
-- newest version already has or it drops that key on deploy.
ALTER TABLE config.feature_toggles
    DROP CONSTRAINT IF EXISTS feature_toggles_key_check;
ALTER TABLE config.feature_toggles
    ADD CONSTRAINT feature_toggles_key_check
        CHECK (key IN ('reminders', 'recall_injector', 'weasel_intercept', 'steerer', 'lesson_worker',
                       'harvester', 'db_resync', 'judge_worker', 'session_judge', 'reminder_on_token',
                       'mailbox_redelivery', 'agent_lessons', 'file_read_compression', 'retention_sweep',
                       'proxy_tools', 'channel_link_sweep', 'workspace_capture_registration',
                       'self_identity_reminder', 'workspace_briefing', 'prompt_assistant',
                       'dossier_builder', 'semantic_context', 'turn_guard', 'directive_proposer',
                       'session_end_shadow', 'launch_brief', 'judge_lessons'));

INSERT INTO config.feature_toggles (key, enabled, operator_set) VALUES
    ('judge_lessons', false, false)
ON CONFLICT (key) DO NOTHING;

ALTER TABLE eval.rates DROP CONSTRAINT IF EXISTS rates_family_check;
ALTER TABLE eval.rates
    ADD CONSTRAINT rates_family_check
        CHECK (family IN ('accuracy', 'label_mix', 'base_rate', 'steer_calibration',
                          'pattern_tagging', 'action_effect'));

ALTER TABLE eval.rates DROP CONSTRAINT IF EXISTS eval_rates_family_metric;
ALTER TABLE eval.rates
    ADD CONSTRAINT eval_rates_family_metric CHECK (
        (family = 'accuracy'          AND metric IN ('precision', 'recall')) OR
        (family IN ('label_mix', 'base_rate') AND metric = 'share') OR
        (family = 'steer_calibration' AND metric IN ('precision', 'fire_rate', 'fire_precision', 'fire_recall')) OR
        (family = 'action_effect'     AND metric = 'share'));

-- action_effect rows (store/persistence/improvement/effect.rs) use population
-- values before/after (a lesson or directive, split at its promotion moment)
-- and holdback/treated (a guard question's arm) — none of which the original
-- population CHECK admits.
ALTER TABLE eval.rates DROP CONSTRAINT IF EXISTS rates_population_check;
ALTER TABLE eval.rates
    ADD CONSTRAINT rates_population_check
        CHECK (population IN ('real_turns', 'replied_real_turns', 'real_session_ends',
                              'steerer_evaluations', 'labelled_turns',
                              'before', 'after', 'holdback', 'treated'));

ALTER TABLE config.change_log DROP CONSTRAINT IF EXISTS change_log_subject_kind_check;
ALTER TABLE config.change_log
    ADD CONSTRAINT change_log_subject_kind_check
    CHECK (subject_kind IN ('setting', 'toggle', 'steerer', 'build', 'promotion',
                            'guard_threshold', 'directive_decision', 'lesson_decision'));

-- One row per project a directive proposal's approval writes a lesson into
-- (JC18 J6 / PLAN 2.2, owned by the approve handler in
-- store/persistence/cross_session/approve.rs). project_id is denormalized
-- from the lesson for a direct per-project listing without a join back
-- through memory.lessons; ON DELETE SET NULL matches directive_proposals'
-- own project_id rule (a proposal's decision history outlives its project).
CREATE TABLE IF NOT EXISTS memory.directive_lessons (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    proposal_id  bigint NOT NULL REFERENCES memory.directive_proposals(id) ON DELETE CASCADE,
    lesson_id    bigint NOT NULL REFERENCES memory.lessons(id) ON DELETE CASCADE,
    project_id   bigint REFERENCES projects.projects(id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT uq_directive_lessons_proposal_lesson UNIQUE (proposal_id, lesson_id)
);

COMMENT ON TABLE memory.directive_lessons IS
    'One row per (directive proposal, lesson) the approval wrote (one lesson per target project, JC18 J6). Read by the improvement actions feed to show which lessons a directive produced.';

CREATE INDEX IF NOT EXISTS idx_directive_lessons_proposal
    ON memory.directive_lessons (proposal_id);
CREATE INDEX IF NOT EXISTS idx_directive_lessons_lesson
    ON memory.directive_lessons (lesson_id);
CREATE INDEX IF NOT EXISTS idx_directive_lessons_project
    ON memory.directive_lessons (project_id) WHERE project_id IS NOT NULL;

-- One row per turn_verdicts row the lesson proposer has examined, whether or
-- not it produced or reinforced a lesson (PLAN 2.1). verdict_id is UNIQUE:
-- it is both the anti-join key ("not yet in memory.lesson_proposal_evidence")
-- the selection query in store/persistence/lessons/proposals.rs uses to skip
-- an already-examined verdict, and the one-attempt-per-verdict invariant.
-- lesson_id is NULL for a verdict the proposer looked at but did not turn
-- into or reinforce a lesson (confidence below threshold at examination
-- time, no resolved correcting message, or the model's own rule = null); an
-- id when it created a new raw lesson or reinforced an existing one
-- (consolidation_count on that lesson incremented in the same statement,
-- store/persistence/lessons/proposals.rs).
CREATE TABLE IF NOT EXISTS memory.lesson_proposal_evidence (
    id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    lesson_id         bigint REFERENCES memory.lessons(id) ON DELETE CASCADE,
    verdict_id        bigint NOT NULL UNIQUE REFERENCES memory.turn_verdicts(id) ON DELETE CASCADE,
    operator_turn_id  bigint NOT NULL REFERENCES memory.operator_turns(id) ON DELETE CASCADE,
    similar_corrections jsonb NOT NULL DEFAULT '[]'::jsonb,
    model             text NOT NULL,
    cost_usd          double precision,
    generation_id     text,
    reason            text NOT NULL DEFAULT '',
    created_at        timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE memory.lesson_proposal_evidence IS
    'One row per corrected verdict the lesson proposer examined. lesson_id NULL means it examined the verdict and wrote no lesson (below confidence, no resolved correction, or the model judged the correction task-specific); set means it created or reinforced that lesson. similar_corrections holds the up-to-three earlier corrections (semantic::Retriever::neighbours, kind Correction) the proposer read before deciding, with their existing lessons if any, exactly as read. Named similar_corrections, not similar: SIMILAR is a reserved PostgreSQL keyword (the SIMILAR TO operator) and cannot be used unquoted as a column identifier.';
COMMENT ON COLUMN memory.lesson_proposal_evidence.reason IS
    'The model''s own stated reason for its decision (wrote a lesson, reinforced one, or wrote neither), shown verbatim as the rationale on the lesson detail page. Default '''' keeps pre-existing rows valid; the write path always supplies the model''s actual text going forward.';
COMMENT ON COLUMN memory.lesson_proposal_evidence.verdict_id IS
    'UNIQUE: the anti-join key the candidate-selection query uses so a verdict is examined at most once, and the fact this table records exactly one attempt per verdict.';

CREATE INDEX IF NOT EXISTS idx_lesson_proposal_evidence_lesson
    ON memory.lesson_proposal_evidence (lesson_id) WHERE lesson_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_lesson_proposal_evidence_turn
    ON memory.lesson_proposal_evidence (operator_turn_id);

-- llm.calls.role and llm.daily_spend.role both gain 'lesson_proposer'
-- (crates/mike/src/lesson_proposer/, this JC21 package): the proposer's own
-- OpenRouter call needs to be attributed and billed like every other
-- caller. Both CHECKs are widened together — llm.daily_spend's AFTER INSERT
-- trigger on llm.calls (trg_llm_calls_daily_spend,
-- 20260928110000_llm_daily_spend_rollup.sql) copies NEW.role verbatim, so a
-- role llm.calls accepts but llm.daily_spend does not would fail every
-- lesson_proposer call's insert at the trigger, not just reject the read.
ALTER TABLE llm.calls DROP CONSTRAINT IF EXISTS calls_role_check;
ALTER TABLE llm.calls
    ADD CONSTRAINT calls_role_check
        CHECK (role IN ('steerer', 'session_judge', 'judge_worker', 'lesson_relations',
                        'eval', 'lesson_proposer'));

ALTER TABLE llm.daily_spend DROP CONSTRAINT IF EXISTS daily_spend_role_check;
ALTER TABLE llm.daily_spend
    ADD CONSTRAINT daily_spend_role_check
        CHECK (role IN ('steerer', 'session_judge', 'judge_worker', 'lesson_relations',
                        'eval', 'lesson_proposer'));

-- The labeller's third work queue, beside JC14's idx_evaluations_unlabelled_shadow and JC17's
-- idx_evaluations_unlabelled_holdback: live pass rows admitted by CANDIDATES_SQL
-- (store/persistence/steering/guard/labels.rs) when steerer.turn_guard.label_live_passes is on
-- (PLAN 2.3.1). Without it the added OR-branch fell back to a SeqScan of steering.evaluations
-- (P4 db-perf-check: 0.571ms -> 6.255ms regression on the BitmapOr plan).
CREATE INDEX IF NOT EXISTS idx_evaluations_unlabelled_live_pass
    ON steering.evaluations (dispatched_at)
    WHERE outcome_label IS NULL AND mode = 'live' AND status = 'resolved_pass' AND arm IS NULL;
