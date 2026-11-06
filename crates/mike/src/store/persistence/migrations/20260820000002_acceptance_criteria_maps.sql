-- GitHub issue #152 — acceptance criteria authored at intake, from the
-- operator's words.
--
-- The false-done judge has nothing authoritative to grade against unless
-- the session happens to be threaded. `thread_criteria.rs` (#119) reads an
-- operator-approved `<acceptance-criteria>` block out of a thread worklog,
-- which is exactly the right principle — but it needs a worklog to exist
-- and the session text to mention its slug. Every other session is graded
-- against a checklist the session itself generated from its own reading of
-- the ask, so a session that misread the ask is graded against its
-- misreading and passes.
--
-- These two tables hold a map authored by a worker that never sees the
-- session's own restatement.
--
-- WHY TWO TABLES, AND WHY A VERSION
--
-- The operator's first message is rarely the complete ask: clarification
-- arrives and discovery changes scope. So the map is re-authored on every
-- operator turn against the accumulated operator-authored text, and each
-- authoring is a new `version` rather than an overwrite. The newest
-- version is what a stop is graded against; the older ones are what make
-- the change visible (#152 AC4 asks for the diff, which needs both sides
-- to still exist).
--
-- Criteria are rows rather than a jsonb blob because each one carries its
-- own provenance and has to be individually addressable — `inferred` items
-- are the ones worth the operator's eye, and a UI that highlights them
-- needs to select them.
--
-- PROVENANCE IS NOT DECORATION
--
-- `operator` means the criterion restates something the operator actually
-- said, and `operator_span` quotes the words it came from. `inferred`
-- means the worker supplied it from repo context. The distinction is the
-- point of the whole ticket: a map with no provenance is indistinguishable
-- from the session-generated checklist it replaces.

CREATE TABLE IF NOT EXISTS memory.acceptance_criteria_maps (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id   bigint NOT NULL REFERENCES sessions.sessions (id) ON DELETE CASCADE,
    -- 1 for the first authoring, incrementing per re-author. Unique with
    -- session_id so a concurrent double-author cannot silently produce two
    -- rows claiming to be the same version.
    version      integer NOT NULL,
    -- How many ask-ledger entries the worker was shown. Re-authoring when
    -- this has not changed is wasted spend; the writer checks it.
    ask_entry_count integer NOT NULL,
    -- The model that authored it, so a map can be read in the light of
    -- what produced it.
    model        text,
    -- Output tokens billed for this authoring, for #152 AC6's cost
    -- reporting. NULL when the provider did not report usage.
    output_tokens integer,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT acceptance_criteria_maps_version_positive CHECK (version >= 1),
    CONSTRAINT acceptance_criteria_maps_session_version_uniq UNIQUE (session_id, version)
);

COMMENT ON TABLE memory.acceptance_criteria_maps IS
    'Issue #152: one authoring of a session''s acceptance criteria, from the '
    'operator''s accumulated words only. Re-authored per operator turn as a new '
    'version rather than overwritten, so the change between versions stays visible.';

CREATE TABLE IF NOT EXISTS memory.acceptance_criteria (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    map_id        bigint NOT NULL REFERENCES memory.acceptance_criteria_maps (id) ON DELETE CASCADE,
    ord           integer NOT NULL,
    criterion     text NOT NULL,
    -- Closed vocabulary. `operator` restates something the operator said,
    -- with the words in `operator_span`; `inferred` came from repo context
    -- and is what an operator should look at first.
    provenance    text NOT NULL,
    -- The operator's own words this criterion rests on. Required for
    -- `operator`, meaningless for `inferred` — enforced below rather than
    -- left to the writer, because a provenance claim with nothing behind
    -- it is worse than no claim.
    operator_span text,
    CONSTRAINT acceptance_criteria_provenance_check
        CHECK (provenance IN ('operator', 'inferred')),
    CONSTRAINT acceptance_criteria_operator_span_backs_the_claim
        CHECK (
            (provenance = 'operator' AND operator_span IS NOT NULL AND length(trim(operator_span)) > 0)
            OR (provenance = 'inferred' AND operator_span IS NULL)
        ),
    CONSTRAINT acceptance_criteria_non_empty CHECK (length(trim(criterion)) > 0),
    CONSTRAINT acceptance_criteria_map_ord_uniq UNIQUE (map_id, ord)
);

COMMENT ON TABLE memory.acceptance_criteria IS
    'Issue #152: one criterion within a map. `provenance` says whether the '
    'operator asked for it or the worker inferred it; an inferred criterion is '
    'the one worth an operator''s eye. Issue #153 requires each criterion name '
    'the state of the SURFACE after the change, never the artifact added.';

-- The only read pattern: newest map for a session, then its criteria in
-- order. The maps unique constraint already covers (session_id, version),
-- which serves the newest-version lookup as a backward index scan; the
-- criteria table needs its own index for the join.
CREATE INDEX IF NOT EXISTS idx_acceptance_criteria_map_ord
    ON memory.acceptance_criteria (map_id, ord);
