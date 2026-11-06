--
-- memory.turn_verdicts: every judgement of an operator turn, append-only.
--
-- Why this exists. Today a turn carries exactly one verdict, written onto
-- memory.operator_turns by stamp_turn_judgment (operator_turns/judgment.rs)
-- and only while outcome_id is still 'unknown'. What the judge saw is not
-- kept next to what it said, the rubric version it stamps is a compiled
-- constant even when the prompt was overridden from /settings, and a
-- second judge (a candidate in shadow, an offline eval run) has nowhere to
-- write. The fleet-mirror audit (night-watch-notes threads/fleet-mirror,
-- 2026-09-23) found stored `corrected` catching 6.8% of real corrections in
-- the r3 era and could not reconstruct which prompt produced which label.
--
-- One row per verdict, never updated. `mode` says which judge wrote it:
--   live    the production turn judge (today the r3 rubric),
--   shadow  a candidate judging a sample of live turns (JC10), never
--           projected onto outcome_id and never read by any context
--           builder,
--   eval    an offline `mike eval run` item (JC10), linked from
--           eval.run_items.
-- `projected` records whether this verdict was written into
-- memory.operator_turns.outcome_id by the same statement that inserted it.
--
-- Two verdict shapes share the table. The legacy r3 rubric returns one
-- label (legacy_label); the two-axis rubric (v2) returns reaction and
-- delivery. The CHECK below requires one shape or the other.
--
-- input_text is the exact user message sent to the model, whole. It is kept
-- here and not only in llm.calls.request_body because llm.calls is written
-- fire-and-forget by the usage sink and only while llm.record_usage is on,
-- and because nothing links an llm.calls row back to the verdict it
-- produced. generation_id is OpenRouter's response id, stored as a
-- fingerprint for cross-reference and deliberately not indexed
-- (.claude/rules/sql-set-based.md, same reasoning as llm.calls).
--
-- next_genuine_request_id is the operator message the verdict was judged
-- against, as resolved by memory.turn_facts at as_of. If turn_facts later
-- reclassifies that message, the verdict is stale and the sweeps re-judge:
-- the verdict row stays as history.
--
-- bundle_id is nullable for live verdicts only. JC06's bundle resolver returns
-- no id on a smoke boot or when it fails, and a production verdict must still
-- be recorded; JC08 counts such rows as verdicts without a bundle. Shadow and
-- eval verdicts exist only to be compared by bundle, so they must carry one.
--
-- No foreign key on next_genuine_request_id, for the reason llm.calls gives
-- for its ids: recording a verdict must not fail because a referenced row
-- is missing.

CREATE TABLE IF NOT EXISTS memory.turn_verdicts (
    id                       bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    operator_turn_id         bigint NOT NULL
                               REFERENCES memory.operator_turns(id) ON DELETE CASCADE,
    session_id               bigint NOT NULL
                               REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    mode                     text NOT NULL
                               CHECK (mode IN ('live', 'shadow', 'eval')),
    bundle_id                bigint
                               REFERENCES eval.bundles(id),
    rubric_version           text NOT NULL,
    input_builder_version    text NOT NULL,
    slices                   text[] NOT NULL DEFAULT '{}',
    model                    text NOT NULL,
    source                   text NOT NULL DEFAULT 'llm'
                               CHECK (source IN ('llm', 'deterministic')),
    reaction                 text
                               CHECK (reaction IN ('corrected', 'redirected', 'accepted',
                                                   'no_response', 'unclear')),
    delivery                 text
                               CHECK (delivery IN ('shipped', 'delivered', 'partial',
                                                   'none', 'question', 'not_applicable')),
    legacy_label             text
                               CHECK (legacy_label IN ('accepted', 'shipped', 'corrected',
                                                       'abandoned', 'unknown')),
    confidence               real NOT NULL
                               CHECK (confidence >= 0 AND confidence <= 1),
    rationale                text NOT NULL,
    unmet_ask                text NOT NULL,
    evidence_quality         text NOT NULL
                               CHECK (evidence_quality IN ('verified', 'asserted', 'absent')),
    projected                boolean NOT NULL DEFAULT false,
    next_genuine_request_id  bigint,
    as_of                    timestamptz NOT NULL,
    input_text               text NOT NULL,
    generation_id            text,
    cost_usd                 double precision,
    failure_tags             text[],
    severity                 text
                               CONSTRAINT turn_verdicts_severity_check
                               CHECK (severity IN ('none', 'minor', 'major', 'severe')),
    prompt_tokens            integer,
    completion_tokens        integer,
    latency_ms               integer,
    created_at               timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT turn_verdicts_shape_check CHECK (
        (reaction IS NOT NULL AND delivery IS NOT NULL AND legacy_label IS NULL)
        OR (reaction IS NULL AND delivery IS NULL AND legacy_label IS NOT NULL)
    ),
    CONSTRAINT turn_verdicts_projection_check CHECK (projected = false OR mode = 'live'),
    CONSTRAINT turn_verdicts_bundle_check CHECK (mode = 'live' OR bundle_id IS NOT NULL)
);

-- Per-turn history (GET /api/operator-turns/{id}/verdicts) and the shadow
-- sweep's anti-join ("does this turn already have a shadow verdict from
-- this bundle?"). Leading column is the FK, per the index-every-FK
-- convention.
CREATE INDEX IF NOT EXISTS idx_turn_verdicts_turn_mode_bundle
    ON memory.turn_verdicts (operator_turn_id, mode, bundle_id);

-- Per-session reads (the dossier and the session judge in later packages)
-- and the FK on session_id.
CREATE INDEX IF NOT EXISTS idx_turn_verdicts_session_created
    ON memory.turn_verdicts (session_id, created_at DESC);

-- Accuracy by bundle (JC08 eval.rates) and the shadow agreement report:
-- all verdicts of one bundle and mode inside a time window. Also covers the
-- FK on bundle_id.
CREATE INDEX IF NOT EXISTS idx_turn_verdicts_bundle_mode_created
    ON memory.turn_verdicts (bundle_id, mode, created_at);

COMMENT ON TABLE memory.turn_verdicts IS
    'Every judgement of an operator turn, append-only: live (production '
    'judge), shadow (candidate on a sample, never projected), eval (offline '
    'mike eval run item). outcome_id on memory.operator_turns is a '
    'projection; projected=true marks the verdict that wrote it.';

COMMENT ON COLUMN memory.turn_verdicts.rubric_version IS
    'Code rubric id (for example r3-2026-08-29, v2-2026-09-23). When the '
    'prompt text in force differs from the built-in text of that id, the '
    'value is <id>+<first 12 hex chars of sha256(text)>.';

COMMENT ON COLUMN memory.turn_verdicts.input_text IS
    'The exact user message sent to the model, whole. The system prompt is '
    'identified by rubric_version plus slices plus bundle_id.';

COMMENT ON COLUMN memory.turn_verdicts.generation_id IS
    'OpenRouter response id. A fingerprint for cross-reference, never a '
    'lookup key: deliberately not indexed.';

-- Added 2026-09-24 (JC18's ask, adopted in JC09 §4.3): failure tags and severity, NULL for the
-- measured candidate v2-2026-09-23, whose schema does not ask for them. A rubric that emits them is
-- a new bundle, gated by JC10 before any live or shadow row carries tags.
COMMENT ON COLUMN memory.turn_verdicts.failure_tags IS
    'Failure tags in the labelling-rubric vocabulary, when the verdict''s rubric asks for them. '
    'NULL for v2-2026-09-23 and r3.';
COMMENT ON COLUMN memory.turn_verdicts.severity IS
    'none, minor, major or severe (JC07''s vocabulary), when the rubric asks for it; NULL otherwise.';
