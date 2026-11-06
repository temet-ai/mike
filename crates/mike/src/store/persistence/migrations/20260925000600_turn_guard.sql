-- The turn-end guard: one steerer that asks four questions at turn end (relapse after a
-- correction, an unbacked wait, an authorised hand-back, done without observing the result), each
-- with its own mode.
--
-- Why. JC00d measured that phrase-triggered endings carry no signal: on the 2,111 census turns,
-- endings on a promise or wait were corrected 43.3% of the time, hand-backs 44.5%, reports 41.4%,
-- other endings 39.5%, and the wait and hand-back gates reach only 46% and 36% of the confirmed
-- instances. The guard lets a phrase only choose which turns get a cheap check, and decides from
-- facts mike holds (a live correction, an authorisation, whether a wait is backed, whether the
-- result was looked at, the session class). It is measured in shadow first: nothing it decides is
-- delivered until a question clears its bar (JC17).
--
-- 1. steerers_kind_check gains 'turn_guard'.
-- 2. feature_toggles_key_check gains 'turn_guard'. No toggle row is inserted: an absent row means
--    the code default, which is off (content egress), and a default never rewrites a stored value.
--    The key list is the one the semantic-context migration left, plus 'turn_guard'.
-- 3. steering.question_policies holds one mode per (guard steerer, question). JC17 ALTERs it.
--    authorised_handback never wakes (CHECK). mode defaults to 'shadow' because the guard does
--    nothing until the turn_guard toggle is on and the steerer row is enabled.
-- 4. Seeds: the backing agent row, the steerer row (DISABLED, "rollout": "shadow") and its four
--    policy rows, each keyed on a unique name so a re-run is a no-op. The promotion bars are code
--    constants (steerer/guard/bars.rs), not config.
--
-- Expand-only and idempotent.

ALTER TABLE steering.steerers
    DROP CONSTRAINT IF EXISTS steerers_kind_check;

ALTER TABLE steering.steerers
    ADD CONSTRAINT steerers_kind_check
        CHECK (kind IN ('refocus', 'memory_keeper', 'verifier', 'turn_guard'));

ALTER TABLE config.feature_toggles
    DROP CONSTRAINT IF EXISTS feature_toggles_key_check;

ALTER TABLE config.feature_toggles
    ADD CONSTRAINT feature_toggles_key_check
        CHECK (key IN ('reminders', 'recall_injector', 'weasel_intercept', 'steerer', 'lesson_worker',
                       'harvester', 'db_resync', 'judge_worker', 'session_judge', 'reminder_on_token',
                       'mailbox_redelivery', 'agent_lessons', 'file_read_compression', 'retention_sweep',
                       'proxy_tools', 'channel_link_sweep', 'workspace_capture_registration',
                       'self_identity_reminder', 'workspace_briefing', 'prompt_assistant',
                       'dossier_builder', 'semantic_context', 'turn_guard'));

CREATE TABLE IF NOT EXISTS steering.question_policies (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    steerer_id  bigint NOT NULL
                  REFERENCES steering.steerers(id) ON DELETE CASCADE,
    question    text NOT NULL
                  CHECK (question IN ('relapse', 'unbacked_wait', 'authorised_handback',
                                      'unobserved_done')),
    mode        text NOT NULL DEFAULT 'shadow'
                  CHECK (mode IN ('off', 'shadow', 'append', 'wake')),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    updated_via text NOT NULL DEFAULT 'migration'
                  CHECK (updated_via IN ('migration', 'spa', 'cli')),
    CONSTRAINT uq_question_policies_steerer_question UNIQUE (steerer_id, question),
    CONSTRAINT question_policies_handback_never_wakes
        CHECK (question <> 'authorised_handback' OR mode <> 'wake')
);

COMMENT ON TABLE steering.question_policies IS
    'One mode per (turn guard steerer, question): off, shadow, append or wake. JC16 creates the '
    'table and writes off and shadow only; JC17 promotes to append or wake through its gate and '
    'adds its holdback columns with ALTER TABLE. authorised_handback never wakes (CHECK).';

COMMENT ON COLUMN steering.question_policies.mode IS
    'off: the question is not asked. shadow: asked and recorded, never delivered. append and wake: '
    'delivered (JC17). Nothing is asked while the turn_guard feature toggle is off.';

INSERT INTO agents.agents (name, description)
SELECT 'turn-guard-steerer',
       'Turn guard steerer - asks four questions at turn end (relapse after correction, unbacked '
       'wait, authorised hand-back, done without observing the result), conditioned on session facts'
WHERE NOT EXISTS (SELECT 1 FROM agents.agents WHERE lower(name) = 'turn-guard-steerer');

WITH agent AS (
    SELECT id FROM agents.agents WHERE lower(name) = 'turn-guard-steerer' ORDER BY id LIMIT 1
),
authority AS (
    SELECT id FROM steering.authorities WHERE code = 'command'
),
default_mode AS (
    SELECT id FROM steering.modes WHERE code = 'drive'
)
INSERT INTO steering.steerers
    (name, display_name, description, kind, agent_id,
     authority_id, default_mode_id, model, priority, enabled,
     trigger_point, invocation, pattern, auto_enrol, config, prompt)
SELECT
    'turn-guard',
    'Turn guard',
    'Asks at turn end whether the turn relapsed after a correction, ended on a wait nothing backs, '
    'handed back an authorised step, or claimed done without observing the result. Phrases only '
    'choose which turns are checked; the decision uses session facts. Exempts fleet seats, '
    'templates, agent requests, machine-opened turns and real decisions.',
    'turn_guard',
    agent.id,
    authority.id,
    default_mode.id,
    '',
    100,
    false,
    'turn_end',
    'every_turn',
    NULL,
    true,
    '{"rollout": "shadow", "debounce_turns": 0, "confidence_threshold": 0.7,
      "quiescence_after_secs": 90, "quiescence_question": "unbacked_wait",
      "questions": {
        "relapse":             {"confidence_threshold": 0.75, "floor": 0.60},
        "unbacked_wait":       {"confidence_threshold": 0.75, "floor": 0.60},
        "authorised_handback": {"confidence_threshold": 0.70, "floor": 0.60},
        "unobserved_done":     {"confidence_threshold": 0.70, "floor": 0.60}}}'::jsonb,
    NULL
  FROM agent
  CROSS JOIN authority
  CROSS JOIN default_mode
 WHERE NOT EXISTS (SELECT 1 FROM steering.steerers WHERE name = 'turn-guard');

INSERT INTO steering.question_policies (steerer_id, question)
SELECT st.id, q.question
  FROM steering.steerers st
  CROSS JOIN (VALUES ('relapse'), ('unbacked_wait'), ('authorised_handback'),
                     ('unobserved_done')) AS q(question)
 WHERE st.name = 'turn-guard'
   AND NOT EXISTS (
       SELECT 1 FROM steering.question_policies p
        WHERE p.steerer_id = st.id AND p.question = q.question
   );
