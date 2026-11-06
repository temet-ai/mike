-- Directive proposals (plans/judgement-context/wp/JC18-cross-session-
-- calibration-directives.md, part B). A proposal is a candidate standing
-- directive that mike noticed the operator had to give more than once: the
-- same class of directive restated in correcting messages across sessions,
-- or a hand-back the operator confirmed they had already authorised. It is
-- NOT a lesson. A raw memory.lessons row is promoted and injected within one
-- consolidation pass (lesson_worker/session_consolidation.rs), so nothing
-- here may be written into memory.lessons until the operator approves it in
-- the SPA; that write is job J6's migration and waits for knowledge WP06
-- (#295) and WP08 (#297).
--
-- Evidence rows are references, not copies: the detail view renders the
-- operator's words from the captured request, in full, at read time, and
-- reports a reference whose row has gone as 'gone'. ref_id is polymorphic
-- (its table depends on kind), so it carries no foreign key; the
-- operator_turn_id and session_id columns carry real ones with SET NULL, so
-- retention of a session never deletes a proposal's history.
--
-- The one-open-proposal-per-key rule is a partial UNIQUE index on short
-- text columns (source, scope, project, class, polarity). None of them is a
-- sha, so sql-set-based.md permits it. polarity is JC03's data.polarity
-- (allow or forbid) for repeated_directive, 'allow' for confirmed_handback
-- and 'none' for the tag and pattern sources, so an allow and a forbid
-- directive of one class never merge into one proposal. class is the
-- canonical JC16 step class for the two directive sources (JC18's class map:
-- JC03 'pr' becomes 'open_pr', 'proceed' becomes 'stop_asking').
--
-- project_id is SET NULL on project deletion, not CASCADE: a proposal's
-- decision history must outlive its project. A 'project' proposal whose
-- project was deleted keeps project_id NULL. rollup_id names the
-- eval.rollups generation whose pass proposed it; plain bigint, no FK. Proposal ids flow from INSERT ... RETURNING
-- into the evidence insert in the same CTE chain; nothing re-derives an id.
--
-- source, scope, status and proposed_kind are closed sets. A new value
-- extends the CHECK in a new migration. proposed_kind mirrors the codes of
-- memory.lesson_kinds (correction, convention, warning, preference).

CREATE TABLE IF NOT EXISTS memory.directive_proposals (
    id                  bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uuid                uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    source              text NOT NULL
                          CHECK (source IN ('repeated_directive', 'confirmed_handback',
                                            'recurring_failure_tag', 'recurring_pattern')),
    scope               text NOT NULL CHECK (scope IN ('project', 'no_project', 'operator')),
    project_id          bigint REFERENCES projects.projects(id) ON DELETE SET NULL,
    class               text NOT NULL CHECK (length(class) BETWEEN 1 AND 200),
    polarity            text NOT NULL DEFAULT 'none'
                          CHECK (polarity IN ('allow', 'forbid', 'none')),
    status              text NOT NULL DEFAULT 'open'
                          CHECK (status IN ('open', 'approved', 'rejected', 'superseded', 'expired')),
    proposed_kind       text NOT NULL
                          CHECK (proposed_kind IN ('correction', 'convention', 'warning', 'preference')),
    proposed_text       text NOT NULL,
    evidence_count      integer NOT NULL CHECK (evidence_count > 0),
    session_count       integer NOT NULL CHECK (session_count > 0),
    first_evidence_at   timestamptz NOT NULL,
    last_evidence_at    timestamptz NOT NULL,
    window_days         integer NOT NULL CHECK (window_days > 0),
    rollup_id           bigint NOT NULL,
    proposer_version    text NOT NULL,
    created_at          timestamptz NOT NULL DEFAULT now(),
    decided_at          timestamptz,
    decision_note       text,
    approved_text       text,
    superseded_by       bigint REFERENCES memory.directive_proposals(id) ON DELETE SET NULL,
    CONSTRAINT directive_proposals_scope_project
        CHECK (scope <> 'operator' OR project_id IS NULL),
    CONSTRAINT directive_proposals_polarity_source
        CHECK ((source = 'repeated_directive' AND polarity IN ('allow', 'forbid'))
               OR (source = 'confirmed_handback' AND polarity = 'allow')
               OR (source IN ('recurring_failure_tag', 'recurring_pattern') AND polarity = 'none')),
    CONSTRAINT directive_proposals_evidence_window
        CHECK (first_evidence_at <= last_evidence_at),
    CONSTRAINT directive_proposals_decided
        CHECK ((status IN ('approved', 'rejected')) = (decided_at IS NOT NULL)),
    CONSTRAINT directive_proposals_approved_text
        CHECK ((status = 'approved') = (approved_text IS NOT NULL)),
    CONSTRAINT directive_proposals_superseded
        CHECK ((status = 'superseded') = (superseded_by IS NOT NULL))
);

COMMENT ON TABLE memory.directive_proposals IS
    'Candidate standing directives found by the JC18 proposer (SQL only, no LLM). Reviewed on /memory. Approval (JC18 J6) writes one memory.lessons row per target project through the ordinary lesson path; until then nothing here is injected anywhere.';
COMMENT ON COLUMN memory.directive_proposals.project_id IS
    'The project for scope ''project''; the sentinel ''(no project)'' row for scope ''no_project''; NULL for scope ''operator'', and NULL for a project-scope proposal whose project was later deleted (SET NULL keeps the decision history).';
COMMENT ON COLUMN memory.directive_proposals.proposed_text IS
    'Deterministic draft, no LLM. repeated_directive: the whole operator message (memory.operator_turns.user_prompt) that carried the most recent qualifying directive, verbatim; the directive''s span is kept in the evidence. confirmed_handback: a fixed sentence followed by every quoted authorisation in full. Never truncated.';
COMMENT ON COLUMN memory.directive_proposals.approved_text IS
    'The exact text the operator approved, byte for byte what J6 writes into memory.lessons.text.';

CREATE UNIQUE INDEX IF NOT EXISTS uq_directive_proposals_open_key
    ON memory.directive_proposals (source, scope, COALESCE(project_id, 0), class, polarity)
    WHERE status = 'open';

-- Serves the rejection-cooldown anti-join (same key, rejected recently).
CREATE INDEX IF NOT EXISTS idx_directive_proposals_key_decided
    ON memory.directive_proposals (source, scope, project_id, class, polarity, decided_at DESC)
    WHERE status = 'rejected';

-- Serves the /memory list (status filter, newest first) and the max_open count.
CREATE INDEX IF NOT EXISTS idx_directive_proposals_status_created
    ON memory.directive_proposals (status, created_at DESC);

CREATE TABLE IF NOT EXISTS memory.directive_proposal_evidence (
    proposal_id       bigint NOT NULL REFERENCES memory.directive_proposals(id) ON DELETE CASCADE,
    kind              text NOT NULL
                        CHECK (kind IN ('dossier_entry', 'turn_verdict', 'evaluation', 'session_end')),
    ref_id            bigint NOT NULL,
    operator_turn_id  bigint REFERENCES memory.operator_turns(id) ON DELETE SET NULL,
    session_id        bigint REFERENCES sessions.sessions(id) ON DELETE SET NULL,
    observed_at       timestamptz NOT NULL,
    detail            jsonb NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (proposal_id, kind, ref_id)
);

COMMENT ON TABLE memory.directive_proposal_evidence IS
    'Every item that made a directive proposal qualify, as a reference (kind + ref_id). kind names the table: dossier_entry -> memory.dossier_entries, turn_verdict -> memory.turn_verdicts, evaluation -> steering.evaluations, session_end -> session_judges.verdicts. detail holds facts copied at proposal time for display, such as a directive''s class, polarity and span in the operator''s message; the message text itself is read at display time, in full.';

CREATE INDEX IF NOT EXISTS idx_directive_proposal_evidence_turn
    ON memory.directive_proposal_evidence (operator_turn_id)
    WHERE operator_turn_id IS NOT NULL;

-- Feature toggle directive_proposer (global, live-disable, default OFF).
-- The proposer is local SQL and sends nothing off the machine, so
-- default-off-mutation-and-egress.md would allow either default. It ships
-- OFF because the catalogue fixes it so (JC18: "off by default"), because an
-- approved proposal becomes injected text with one click (after J6), and
-- because its main source, JC03's keyword extractor directives-v1, has no
-- measured precision yet.
-- operator_set stays false so the code default wins until the operator
-- flips it.
--
-- config.feature_toggles.key carries a closed CHECK that each new toggle
-- re-creates in full (last on 2026-09-23: 20260922150001_prompt_assistant_
-- toggle.sql, 20 keys). BUILD-TIME STEP: regenerate the list below from
-- main's latest copy of this constraint and append directive_proposer. Other
-- JC packages add toggles too; a list copied from this draft would silently
-- drop theirs.

ALTER TABLE config.feature_toggles
    DROP CONSTRAINT IF EXISTS feature_toggles_key_check;
ALTER TABLE config.feature_toggles
    ADD CONSTRAINT feature_toggles_key_check
        CHECK (key IN ('reminders', 'recall_injector', 'weasel_intercept', 'steerer', 'lesson_worker',
                       'harvester', 'db_resync', 'judge_worker', 'session_judge', 'reminder_on_token',
                       'mailbox_redelivery', 'agent_lessons', 'file_read_compression', 'retention_sweep',
                       'proxy_tools', 'channel_link_sweep', 'workspace_capture_registration',
                       'self_identity_reminder', 'workspace_briefing', 'prompt_assistant',
                       'dossier_builder', 'semantic_context', 'turn_guard', 'directive_proposer'));

INSERT INTO config.feature_toggles (key, enabled, operator_set) VALUES
    ('directive_proposer', false, false)
ON CONFLICT (key) DO NOTHING;
