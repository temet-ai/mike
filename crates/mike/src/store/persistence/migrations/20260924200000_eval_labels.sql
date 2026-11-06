-- JC07 (judgement-context program): eval.labels, the labelled set.
--
-- Why: mike's stored turn labels are wrong and nothing measures them (stored `corrected`
-- recall on real operator turns 24.8% before 2026-08-31, 6.8% after; `abandoned` a real
-- frustration close in 27 of 626 labels), and the operator's own corrections of labels
-- were never captured (fleet-mirror analysis.md, issue-drafts.md Drafts 8 and 9).
-- eval.labels holds labels from four tiers (gold: the operator's clicks in the SPA;
-- relayed: an agent quoting the operator; auto_shadow: next-message labels on shadow
-- evaluations; seed: imported double-blind or single-model labels) with a dev or holdout
-- split fixed per turn at its first label.
--
-- Append-only: a correction is a new row that supersedes the old one; a withdrawal is a
-- row with action = 'withdraw'. eval.labels_as_of(T) is the one definition of "current".
--
-- No FK to memory.operator_turns or sessions.sessions, deliberately: the TTL sweep
-- (store/persistence/startup_health.rs:21-80) deletes sessions and cascades to turns; a
-- CASCADE would silently delete gold labels, a RESTRICT would fail the whole sweep.
-- session_uuid and operator_turn_id are kept on the row instead.
--
-- No lock_timeout is set here: this file creates new objects only and alters no
-- existing table, so the migration connection's configured timeout
-- (PROXY_MIGRATION_LOCK_TIMEOUT_SECS, store/persistence/pool.rs:15-21) applies.

CREATE SCHEMA IF NOT EXISTS eval;

CREATE TABLE IF NOT EXISTS eval.axis_labels (
    axis          text    NOT NULL,
    label         text    NOT NULL,
    subject_kinds text[]  NOT NULL,
    sort_order    integer NOT NULL,
    description   text    NOT NULL,
    PRIMARY KEY (axis, label)
);

COMMENT ON TABLE eval.axis_labels IS
    'Label vocabulary per axis. A table rather than a CHECK so a new failure pattern (JC20 clustering) is a data insert, not a migration.';

CREATE TABLE IF NOT EXISTS eval.turn_splits (
    operator_turn_id bigint      PRIMARY KEY,
    split            text        NOT NULL CHECK (split IN ('dev', 'holdout')),
    split_reason     text        NOT NULL CHECK (split_reason IN ('seed_import', 'holdout_import', 'hash')),
    assigned_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT turn_splits_turn_split_key UNIQUE (operator_turn_id, split)
);

COMMENT ON TABLE eval.turn_splits IS
    'The dev or holdout split of a turn, fixed at its first label and shared by every subject of that turn. Holdout labels never enter any context, prompt or tuning export (design-tuning-loop.md invariant 3).';

CREATE TABLE IF NOT EXISTS eval.import_batches (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source_key  text        NOT NULL UNIQUE,
    manifest    jsonb       NOT NULL,
    counts      jsonb       NOT NULL,
    imported_at timestamptz NOT NULL DEFAULT now(),
    imported_by text        NOT NULL
);

COMMENT ON COLUMN eval.import_batches.source_key IS
    'A plain name for the imported run (for example fleet-mirror-7772e9a0). Never a hash; file SHA-256 values are data inside manifest.';

-- Added 2026-09-24 (JC02 §14 Q2 and JC07 §14 Q9, owner decided: JC07's importer, read by JC08's
-- eval.turn_population_v). Sessions or turns that are not operator work by fleet-mirror's synthetic
-- rule (threads/fleet-mirror/analyse.py:35-50): the 2026-07-27 skill-eval batch, scratch-project
-- sessions and probe prompts. A NULL operator_turn_id excludes the whole session. Written once by
-- `mike eval import fleet-mirror --execute` in the same transaction as the labels.
CREATE TABLE IF NOT EXISTS eval.population_exclusions (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id       bigint      NOT NULL,
    operator_turn_id bigint,
    reason           text        NOT NULL
                     CHECK (reason IN ('skill_eval_batch', 'scratch_project', 'probe_prompt')),
    import_batch_id  bigint      NOT NULL REFERENCES eval.import_batches(id),
    created_at       timestamptz NOT NULL DEFAULT now(),
    withdrawn_at     timestamptz
);

CREATE INDEX IF NOT EXISTS population_exclusions_session_idx
    ON eval.population_exclusions (session_id, operator_turn_id);

CREATE TABLE IF NOT EXISTS eval.labels (
    id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    subject_kind      text        NOT NULL
                      CHECK (subject_kind IN ('turn', 'session_end', 'evaluation', 'intervention')),
    subject_id        bigint      NOT NULL,
    operator_turn_id  bigint      NOT NULL,
    session_id        bigint      NOT NULL,
    session_uuid      uuid        NOT NULL,
    axis              text        NOT NULL,
    label             text        NOT NULL,
    action            text        NOT NULL DEFAULT 'assert'
                      CHECK (action IN ('assert', 'withdraw')),
    tier              text        NOT NULL
                      CHECK (tier IN ('gold', 'relayed', 'auto_shadow', 'seed')),
    method            text        NOT NULL
                      CHECK (method IN ('operator_click', 'operator_confirmed_relay', 'relayed_quote',
                                        'next_message_auto', 'double_blind_adjudicated', 'single_model')),
    split             text        NOT NULL,
    tags              text[]      NOT NULL DEFAULT '{}',
    severity          text        CHECK (severity IN ('none', 'minor', 'major', 'severe')),
    confidence        text,
    evidence          text,
    note              text,
    source            text        NOT NULL,
    provenance        jsonb       NOT NULL DEFAULT '{}'::jsonb,
    inclusion_prob    double precision CHECK (inclusion_prob > 0 AND inclusion_prob <= 1),
    verdict_snapshot  jsonb,
    verdict_bundle_id bigint,
    import_batch_id   bigint      REFERENCES eval.import_batches(id),
    supersedes_id     bigint      REFERENCES eval.labels(id),
    created_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT labels_vocabulary_fk FOREIGN KEY (axis, label)
        REFERENCES eval.axis_labels (axis, label),
    CONSTRAINT labels_split_fk FOREIGN KEY (operator_turn_id, split)
        REFERENCES eval.turn_splits (operator_turn_id, split),
    CONSTRAINT labels_turn_subject_is_turn
        CHECK (subject_kind NOT IN ('turn', 'session_end') OR subject_id = operator_turn_id),
    CONSTRAINT labels_tier_method CHECK (
        (tier = 'gold'        AND method IN ('operator_click', 'operator_confirmed_relay')) OR
        (tier = 'relayed'     AND method = 'relayed_quote') OR
        (tier = 'auto_shadow' AND method = 'next_message_auto') OR
        (tier = 'seed'        AND method IN ('double_blind_adjudicated', 'single_model'))),
    CONSTRAINT labels_seed_has_batch CHECK (tier <> 'seed' OR import_batch_id IS NOT NULL)
);

COMMENT ON TABLE eval.labels IS
    'Append-only labelled set. Read current labels through eval.labels_as_of(T) or eval.labels_current_v, never by re-deriving the supersede and withdraw rules.';
COMMENT ON COLUMN eval.labels.subject_id IS
    'memory.operator_turns.id for turn and session_end (the session''s ending turn, as in fleet-mirror session-ends.json); steering.evaluations.id or steering.interventions.id otherwise.';
COMMENT ON COLUMN eval.labels.verdict_snapshot IS
    'Mike''s verdict on the subject when a gold or relayed label was written (outcome, judge model, rubric, bundle, time), so the label measures the verdict it corrected even after a re-judge.';
COMMENT ON COLUMN eval.labels.inclusion_prob IS
    'Sampling probability for Horvitz-Thompson weighting (JC08). 1 for full populations; 25/N per stratum for the fleet-mirror ground truth in the real-turn scope; NULL when unknown.';

CREATE INDEX IF NOT EXISTS labels_subject_idx
    ON eval.labels (subject_kind, subject_id, axis, id DESC);
CREATE INDEX IF NOT EXISTS labels_session_idx
    ON eval.labels (session_uuid, created_at);
CREATE INDEX IF NOT EXISTS labels_turn_idx
    ON eval.labels (operator_turn_id);
CREATE INDEX IF NOT EXISTS labels_tier_created_idx
    ON eval.labels (tier, created_at DESC, id DESC);
-- A row can be superseded once. Two concurrent saves on the same subject and axis would
-- otherwise both supersede the same current row and both stay current. The writers also
-- serialise on a per-subject advisory lock (so the first label of an axis, which
-- supersedes nothing, is serialised too) and map a violation of this index to 409
-- "label changed; reload". The index also serves labels_as_of's NOT EXISTS anti-join.
CREATE UNIQUE INDEX IF NOT EXISTS labels_single_successor
    ON eval.labels (supersedes_id) WHERE supersedes_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS labels_import_batch_idx
    ON eval.labels (import_batch_id) WHERE import_batch_id IS NOT NULL;

CREATE OR REPLACE FUNCTION eval.labels_as_of(p_at timestamptz)
RETURNS SETOF eval.labels
LANGUAGE sql STABLE AS $$
    SELECT l.*
      FROM eval.labels l
     WHERE l.created_at <= p_at
       AND l.action = 'assert'
       AND NOT EXISTS (
           SELECT 1
             FROM eval.labels s
            WHERE s.supersedes_id = l.id
              AND s.created_at <= p_at
       )
$$;

COMMENT ON FUNCTION eval.labels_as_of(timestamptz) IS
    'The labels current at a moment: asserted, created by then, and not superseded (or withdrawn) by then. The one definition every consumer uses (JC08, JC09, JC10, JC12).';

CREATE OR REPLACE VIEW eval.labels_current_v AS
    SELECT * FROM eval.labels_as_of('infinity'::timestamptz);

-- Vocabulary. Reaction and delivery follow the candidate judge's two axes
-- (fleet-mirror judge-candidate-system-prompt.txt); session_end follows
-- session-end-rubric.md; failure_tag is the 21 audited mirror patterns (mirror.json)
-- plus the two cross-turn patterns found in frustration closes (session-ends.json).
INSERT INTO eval.axis_labels (axis, label, subject_kinds, sort_order, description) VALUES
    ('reaction', 'corrected',    '{turn}', 1, 'The next genuine message says or implies the assistant got something wrong in this turn: a mistake, a misread ask, a false or unchecked claim, an ignored instruction, wasted effort, or work not done. Re-asking and challenging questions count.'),
    ('reaction', 'redirected',   '{turn}', 2, 'The operator changes direction, scope or priority for reasons that are not the assistant''s fault.'),
    ('reaction', 'accepted',     '{turn}', 3, 'The operator builds on the turn, approves it, thanks, answers a fair question, or gives the next step in the same line of work.'),
    ('reaction', 'no_response',  '{turn}', 4, 'No genuine message followed. Not a verdict, and never "abandoned".'),
    ('reaction', 'unclear',      '{turn}', 5, 'A next message exists but cannot be read as any of the above without guessing.'),
    ('delivery', 'shipped',        '{turn}', 1, 'Delivered what was asked and an irreversible delivery action happened in this turn: commit, push, pull request, merge, deploy or release.'),
    ('delivery', 'delivered',      '{turn}', 2, 'Reports the asked-for work done or the question answered, with no irreversible action.'),
    ('delivery', 'partial',        '{turn}', 3, 'Some of the ask is done and the rest is explicitly left, blocked, deferred or still running.'),
    ('delivery', 'none',           '{turn}', 4, 'No real work: refused, only planned, cut short before any work, or only chatter.'),
    ('delivery', 'question',       '{turn}', 5, 'The main output is a question or decision put to the operator, with little of the ask done.'),
    ('delivery', 'not_applicable', '{turn}', 6, 'The prompt asks for no work: thanks, a status ping, an automated event.'),
    ('session_end', 'done',                  '{session_end}', 1, 'The session reached a natural end and nothing shows dissatisfaction. The default.'),
    ('session_end', 'closed_in_frustration', '{session_end}', 2, 'The operator gave up: corrections or frustration that the final turn did not resolve.'),
    ('session_end', 'cut_off_neutral',       '{session_end}', 3, 'Work unfinished, but nothing shows frustration: still running, carried into a new session, a context limit, or the operator stepped away.'),
    ('session_end', 'fleet_seat',            '{session_end}', 4, 'A fleet seat closed by its coordinator, with no operator typing in it.'),
    ('session_end', 'unclear',               '{session_end}', 5, 'The record cannot support any of the above without guessing.'),
    ('fact_own_prompt_corrects_prior', 'true',  '{turn}', 1, 'This turn''s own prompt corrects an earlier turn.'),
    ('fact_own_prompt_corrects_prior', 'false', '{turn}', 2, 'This turn''s own prompt does not correct an earlier turn.'),
    ('fact_assistant_self_correction', 'true',  '{turn}', 1, 'The assistant fixed its own earlier mistakes within the turn, beyond routine test-fail-fix.'),
    ('fact_assistant_self_correction', 'false', '{turn}', 2, 'No such self-correction.'),
    ('fact_irreversible_action', 'true',  '{turn}', 1, 'The turn took an irreversible action (commit, push, merge, deploy, release).'),
    ('fact_irreversible_action', 'false', '{turn}', 2, 'No irreversible action.'),
    ('steer_valid', 'right',   '{evaluation,intervention}', 1, 'The steer, or the decision not to steer, was right for this turn.'),
    ('steer_valid', 'wrong',   '{evaluation,intervention}', 2, 'The steer, or the decision not to steer, was wrong for this turn.'),
    ('steer_valid', 'unclear', '{evaluation,intervention}', 3, 'Cannot tell without guessing.'),
    ('failure_tag', 'empty-turn',                              '{turn,session_end}',  1, 'No visible reply or work, and the operator repeats the same request.'),
    ('failure_tag', 'stalled-on-promise-wait-or-silence',      '{turn,session_end}',  2, 'The turn ends on an announced next step, a progress fragment, or an idle wait instead of driving the work forward.'),
    ('failure_tag', 'handed-back-actions-or-decisions',        '{turn,session_end}',  3, 'Hands back something the assistant could do itself: asks permission for, or flags as a decision, something already authorised or within its remit.'),
    ('failure_tag', 'claimed-done-without-checking',           '{turn,session_end}',  4, 'Reports its deliverable fixed, verified or working without looking at the result or running it.'),
    ('failure_tag', 'asserted-unchecked-or-invented-facts',    '{turn,session_end}',  5, 'States as fact something it did not check or that no source supports.'),
    ('failure_tag', 'invented-content-beyond-source',          '{turn,session_end}',  6, 'Fills a deliverable with specifics neither the supplied material nor the operator gave.'),
    ('failure_tag', 'ignored-references-and-required-process', '{turn,session_end}',  7, 'Works without, or against, the references, design system, libraries or process the operator supplied or the project has.'),
    ('failure_tag', 'repeated-corrected-mistake',              '{turn,session_end}',  8, 'Brings back something the operator had explicitly corrected, banned, removed or settled.'),
    ('failure_tag', 'fixed-too-narrowly',                      '{turn,session_end}',  9, 'Does a narrow slice of the ask and reports or implies the whole job is done.'),
    ('failure_tag', 'filed-instead-of-fixed',                  '{turn,session_end}', 10, 'Lists defects of the current task as follow-ups or out of scope instead of fixing them.'),
    ('failure_tag', 'work-not-landed',                         '{turn,session_end}', 11, 'Told to commit, push, merge or deploy, leaves the work unlanded and still reports closure.'),
    ('failure_tag', 'delegated-and-stopped-owning',            '{turn,session_end}', 12, 'Hands work to subagents or peers, then waits or relays without checking progress or quality.'),
    ('failure_tag', 'changed-untouched-things',                '{turn,session_end}', 13, 'Alters, removes or reverts working code, config or state outside the request.'),
    ('failure_tag', 'added-unrequested-content',               '{turn,session_end}', 14, 'Adds net-new things nobody asked for.'),
    ('failure_tag', 'drifted-onto-tangent',                    '{turn,session_end}', 15, 'Spends the turn on something outside the current ask, so the request is not advanced.'),
    ('failure_tag', 'misread-the-ask',                         '{turn,session_end}', 16, 'Acts on a wrong reading of the operator''s message.'),
    ('failure_tag', 'unreadable-prose',                        '{turn,session_end}', 17, 'Writes narrated history, dense private jargon, or its own undefined labels and ids.'),
    ('failure_tag', 'clickbait-wording',                       '{turn,session_end}', 18, 'Writes in punchy, dramatic or alarmist headline style.'),
    ('failure_tag', 'broke-standing-code-and-commit-rules',    '{turn,session_end}', 19, 'Code, commits or merge requests break the operator''s standing rules.'),
    ('failure_tag', 'undeliverable-output',                    '{turn,session_end}', 20, 'Says an artifact was produced but gives it in a form the operator cannot open or see.'),
    ('failure_tag', 'slow-verification-loops',                 '{turn,session_end}', 21, 'Spends long stretches re-running builds, tests or checks while the deliverable stays unfinished.'),
    ('failure_tag', 'relapsed-right-after-correction',         '{turn,session_end}', 22, 'After the operator corrects a behaviour, the next turn or turns repeat the same behaviour, often in nearly the same words.'),
    ('failure_tag', 'acknowledged-correction-without-acting',  '{turn,session_end}', 23, 'After a correction, the turn only acknowledges or restates it and makes no change toward it.'),
    ('failure_tag', 'none',                                    '{turn,session_end}', 99, 'A corrected turn to which no pattern applied.')
ON CONFLICT (axis, label) DO NOTHING;
