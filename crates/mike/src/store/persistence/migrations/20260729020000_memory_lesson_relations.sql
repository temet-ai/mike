-- Directional lesson-relationship graph: "B updates A" (supersedes / refines
-- / recurs / contradicts), proposed by a comparison pass and adjudicated
-- before being trusted. This is never a deletion mechanism -- the actual
-- archive/delete pipeline is memory.consolidation_actions
-- (20260729010000_memory_consolidation_audit.sql); a relation here only
-- records that a relationship was proposed and how it was decided.
--
-- Cosine distance nominates a CANDIDATE pair -- which two lessons are close
-- enough to compare -- and nothing more. Direction and kind come only from
-- evidence (quoted spans, artifact refs, timestamps) and adjudication.

CREATE TABLE IF NOT EXISTS memory.lesson_relation_kinds (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.lesson_relation_kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'supersedes'),
    (2, 'refines'),
    (3, 'recurs'),
    (4, 'contradicts')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.lesson_relation_kinds_id_seq', 4);

CREATE TABLE IF NOT EXISTS memory.lesson_relation_methods (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.lesson_relation_methods (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'declared'),
    (2, 'artifact_reference'),
    (3, 'adjudicated'),
    (4, 'operator')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.lesson_relation_methods_id_seq', 4);

CREATE TABLE IF NOT EXISTS memory.lesson_relation_statuses (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.lesson_relation_statuses (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'proposed'),
    (2, 'confirmed'),
    (3, 'rejected'),
    (4, 'contested')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.lesson_relation_statuses_id_seq', 4);

CREATE TABLE IF NOT EXISTS memory.lesson_relations (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uuid uuid NOT NULL DEFAULT gen_random_uuid(),

    -- The newer, superseding/refining lesson.
    from_lesson_id bigint NOT NULL REFERENCES memory.lessons(id),
    -- The older lesson being superseded/refined. No ON DELETE CASCADE on
    -- either side: a relation is never a route by which a lesson row can be
    -- destroyed (default NO ACTION is correct here).
    to_lesson_id bigint NOT NULL REFERENCES memory.lessons(id),

    kind_id bigint NOT NULL REFERENCES memory.lesson_relation_kinds(id),
    method_id bigint NOT NULL REFERENCES memory.lesson_relation_methods(id),
    status_id bigint NOT NULL REFERENCES memory.lesson_relation_statuses(id),

    -- Cosine distance that nominated the pair. Never decides kind,
    -- direction, or status -- see table comment.
    candidate_distance double precision,

    -- Verifiable claims only (quoted spans, artifact refs, timestamps) --
    -- the proposer's free-form reasoning belongs in proposer_rationale.
    evidence_json jsonb NOT NULL DEFAULT '{}'::jsonb,

    proposer_model text,
    proposer_rationale text,
    adjudicator_model text,
    adjudicator_agrees boolean,
    adjudicator_rationale text,
    detector_version text NOT NULL,

    proposed_at timestamptz NOT NULL DEFAULT now(),
    decided_at timestamptz,
    decided_by text,
    created_at timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT lesson_relations_not_self CHECK (from_lesson_id <> to_lesson_id),
    CONSTRAINT lesson_relations_adjudicator_not_proposer
        CHECK (adjudicator_model IS NULL OR proposer_model IS NULL OR adjudicator_model <> proposer_model),
    CONSTRAINT uq_lesson_relations_from_to_kind UNIQUE (from_lesson_id, to_lesson_id, kind_id)
);

COMMENT ON TABLE memory.lesson_relations IS 'Directional "B updates A" graph between lessons (supersedes/refines/recurs/contradicts) -- never a deletion mechanism, only a record that a relationship was proposed and how it was adjudicated. Direction and kind come from evidence_json/proposer_rationale/adjudicator review; candidate_distance only nominated the pair for comparison.';
COMMENT ON CONSTRAINT lesson_relations_adjudicator_not_proposer ON memory.lesson_relations IS 'A model never adjudicates its own proposal.';

CREATE INDEX IF NOT EXISTS idx_lesson_relations_from_lesson_id ON memory.lesson_relations USING btree (from_lesson_id);
CREATE INDEX IF NOT EXISTS idx_lesson_relations_to_lesson_id ON memory.lesson_relations USING btree (to_lesson_id);

-- kind_id/method_id/status_id are NOT indexed: static lookup tables (4 rows
-- each) whose rows are never deleted -- same rationale as
-- 20260729010000_memory_consolidation_audit.sql's consolidation_runs comment.

-- ── lesson_injections backfill idempotency ─────────────────────────────────

-- No dedup step here on purpose: this index is the arbiter for a backfill's
-- ON CONFLICT, not a cleanup pass. If the table already held duplicate
-- (lesson_id, request_id) pairs, building it should fail loudly rather than
-- silently delete rows to make room.
CREATE UNIQUE INDEX IF NOT EXISTS idx_lesson_injections_lesson_request ON memory.lesson_injections (lesson_id, request_id);

COMMENT ON INDEX memory.idx_lesson_injections_lesson_request IS 'ON CONFLICT arbiter for the historical lesson->request backfill, so re-running it cannot double-insert links.';
