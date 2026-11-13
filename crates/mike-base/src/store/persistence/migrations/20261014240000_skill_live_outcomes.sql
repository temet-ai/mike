CREATE TABLE memory.skill_live_outcomes (
    project_id           bigint      NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    folder               text        NOT NULL,
    first_shipped_on     date        NOT NULL,
    window_start         timestamptz NOT NULL,
    window_end           timestamptz NOT NULL,
    loads                integer     NOT NULL CHECK (loads >= 0),
    load_sessions        integer     NOT NULL CHECK (load_sessions >= 0),
    loaded_turns         integer     NOT NULL CHECK (loaded_turns >= 0),
    judged_turns         integer     NOT NULL CHECK (judged_turns >= 0),
    corrected_turns      integer     NOT NULL CHECK (corrected_turns >= 0),
    base_judged_turns    integer     NOT NULL CHECK (base_judged_turns >= 0),
    base_corrected_turns integer     NOT NULL CHECK (base_corrected_turns >= 0),
    retired_at           timestamptz,
    retired_reason       text CHECK (retired_reason IN ('unloaded', 'corrected_above_base')),
    measured_at          timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (project_id, folder, first_shipped_on),
    CHECK (judged_turns <= loaded_turns),
    CHECK (corrected_turns <= judged_turns),
    CHECK (base_corrected_turns <= base_judged_turns),
    CHECK ((retired_at IS NULL) = (retired_reason IS NULL))
);

COMMENT ON TABLE memory.skill_live_outcomes IS
    'How a skill the learning promoter shipped has done in real use. One row per project, skill '
    'folder and shipment (first_shipped_on is the promoter ledger''s first_emitted for it), written '
    'by the promoter on each run through POST /api/learning/skill-outcomes. loads counts Skill tool '
    'uses of that folder in the project since window_start, load_sessions the sessions they came '
    'from, loaded_turns the operator turns that contained a load, judged_turns the loaded turns '
    'whose judge reaction is accepted, corrected or redirected, and corrected_turns those judged '
    'corrected. base_judged_turns and base_corrected_turns are the same counts over all real '
    'operator turns of the project in the same window. The promoter retires a skill that was not '
    'loaded for 30 days (unloaded) or that has at least 10 loads and a corrected rate above the '
    'base rate (corrected_above_base); retired_at and retired_reason record it and the next skill '
    'draft for the topic receives these numbers as author input.';

CREATE VIEW memory.skill_evidence_v AS
WITH latest_draft AS (
    SELECT DISTINCT ON (d.project_id, d.folder)
           d.project_id, d.folder, d.id AS draft_id, d.status, d.revision,
           d.evaluated_at, d.eval
      FROM memory.skill_drafts d
     ORDER BY d.project_id, d.folder, d.lesson_watermark DESC, d.revision DESC),
latest_live AS (
    SELECT DISTINCT ON (o.project_id, o.folder) o.*
      FROM memory.skill_live_outcomes o
     ORDER BY o.project_id, o.folder, o.first_shipped_on DESC)
SELECT p.project_path,
       COALESCE(d.project_id, o.project_id) AS project_id,
       COALESCE(d.folder, o.folder) AS folder,
       d.draft_id,
       d.status AS draft_status,
       d.revision AS draft_revision,
       d.evaluated_at,
       (d.eval ->> 'evaluator_version')::integer AS evaluator_version,
       (d.eval -> 'trigger' ->> 'score')::double precision AS trigger_score,
       (d.eval -> 'recheck' ->> 'score')::double precision AS patched_trigger_score,
       d.eval #>> '{review,decision}' AS review_decision,
       d.eval #>> '{review,reason}' AS review_reason,
       d.eval #>> '{review,overlap}' AS review_overlap,
       d.eval #>> '{review,belongs_in}' AS review_belongs_in,
       (d.eval ->> 'calls')::integer AS eval_calls,
       d.eval ->> 'error' AS eval_error,
       o.first_shipped_on,
       o.window_start,
       o.window_end,
       o.loads,
       o.load_sessions,
       o.loaded_turns,
       o.judged_turns,
       o.corrected_turns,
       o.corrected_turns::double precision / NULLIF(o.judged_turns, 0) AS corrected_rate,
       o.base_judged_turns,
       o.base_corrected_turns,
       o.base_corrected_turns::double precision / NULLIF(o.base_judged_turns, 0) AS base_corrected_rate,
       o.retired_at,
       o.retired_reason,
       o.measured_at
  FROM latest_draft d
  FULL OUTER JOIN latest_live o ON o.project_id = d.project_id AND o.folder = d.folder
  JOIN projects.projects p ON p.id = COALESCE(d.project_id, o.project_id);

COMMENT ON VIEW memory.skill_evidence_v IS
    'One row per project and skill folder: the newest skill draft with its evaluation (the trigger '
    'test score, the reviewer''s decision and reason, the existing skill, rule, agent or memory '
    'file the reviewer named as covering or owning the content, and the model calls it took) '
    'beside the newest '
    'shipment''s live numbers from memory.skill_live_outcomes (loads, the corrected rate on the '
    'turns that loaded the skill, the project''s base corrected rate over the same window, and '
    'whether and why the promoter retired it). A folder with a draft and no shipment, or a '
    'shipment with no draft, has NULLs for the missing half.';
