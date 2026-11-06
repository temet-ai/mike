-- ════════════════════════════════════════════════════════════════════
-- JC20: semantic context (embeddings in the accumulating context)
--
-- Why: mike embeds every closed turn with a local model (nomic-embed-text-v1.5,
-- memory.operator_turn_chunks), but reads the vectors only through project-path
-- recall. JC20 makes them part of the dossier: similar corrections, label-free
-- exchanges, directives and lessons by similarity across projects; a no-LLM
-- pattern tagger against the audited fleet-mirror instances; daily embedding
-- drift beside label_mix_shift. P0 (plans/judgement-context/p0/out/e20-build.txt,
-- i20.txt) found that similarity only means something against a chance floor per
-- facet pair, so floors are stored and versioned with the embedder.
--
-- Shape of the design:
--   - memory.semantic_items: one row per embedded text under one embedder key.
--     Immutable; a rebuild is a new row (builder_version) and a new embedder is a
--     new set of rows (embedder_key), so any past retrieval can be rebuilt.
--     Corrections are NOT stored as items: qualification is joined as of the query
--     moment from eval.labels_as_of / memory.turn_verdicts / session ends.
--   - The text column stores exactly what was embedded (inspection-fidelity layer 1).
--   - memory.semantic_embedders: one row per embedding space, identified by the
--     vectors a fixed canary set returns, not by settings. The stored chunk corpus
--     spans three backend regimes under one model name (2026-07-18..08-26,
--     08-29..09-09, from 09-14 12:23Z; identical texts at cosine 0.88-0.98 across
--     them), and one boundary left no settings row. So a settings-derived key is
--     not enough, and chunk vectors are copied only inside a verified window.
--   - embedder_key is plain text
--     (<model>|<dim>|<chunk_max_chars>|doc-prefix-v1|since:<first_seen_at>),
--     not a hash, so the unique key may include it (.claude/rules/sql-set-based.md).
--   - Anchors (audited mirror instances, operator-named cluster members) carry no
--     session_id, so the session TTL sweep (startup_health.rs:21-91) never removes them.
--     They store their origin session and turn instead, so the tagger can exclude
--     same-session and self matches without a join to rows TTL deletes.
--   - Floors are pinned: status in_force changes only on a new embedder or an
--     explicit promotion; the weekly recompute writes monitoring rows.
--   - Turn items cascade with their session and turn: retention wins over memory.
--   - JC18 J7's shadow picks (memory.semantic_shadow_picks) are not built: J7 is
--     removed from JC18.
--
-- Expand-only. Idempotent. No destructive statement.
-- ════════════════════════════════════════════════════════════════════

SET LOCAL lock_timeout = '5s';

CREATE TABLE IF NOT EXISTS memory.semantic_embedders (
    id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    embedder_key      text        NOT NULL,
    model             text        NOT NULL,
    dim               integer     NOT NULL CHECK (dim > 0),
    chunk_max_chars   integer     NOT NULL CHECK (chunk_max_chars > 0),
    prefix_version    text        NOT NULL,
    urls_seen         text[]      NOT NULL,
    canary_version    text        NOT NULL,
    canary_vectors    jsonb       NOT NULL,
    status            text        NOT NULL DEFAULT 'current',
    first_seen_at     timestamptz NOT NULL DEFAULT now(),
    last_checked_at   timestamptz NOT NULL DEFAULT now(),
    copy_valid_from   timestamptz,
    copy_verify       jsonb,
    CONSTRAINT semantic_embedders_status_check CHECK (status IN ('current', 'retired')),
    CONSTRAINT semantic_embedders_copy_verified
        CHECK ((copy_valid_from IS NULL) = (copy_verify IS NULL)),
    CONSTRAINT uq_semantic_embedders_key UNIQUE (embedder_key)
);

COMMENT ON TABLE memory.semantic_embedders IS
    'JC20. One row per embedding space. Same space = same model, dim, chunk size and prefix version, and every canary vector at cosine >= 0.9999 with the stored one. Checked at worker start, on any change of the embedding service''s config snapshot, on the first call after the circuit breaker closes, and daily. copy_valid_from: operator_turn_chunks embedded at or after it may be copied instead of re-embedded; NULL until mike semantic embedders verify has re-embedded a sample and found every one identical. copy_verify holds that sample''s ids and minimum cosine.';
COMMENT ON COLUMN memory.semantic_embedders.canary_vectors IS
    'The canary set''s vectors, whole: {"version": ..., "texts": [...], "vectors": [[768 floats], ...]}.';

CREATE UNIQUE INDEX IF NOT EXISTS uq_semantic_embedders_current
    ON memory.semantic_embedders ((true)) WHERE status = 'current';

CREATE TABLE IF NOT EXISTS memory.semantic_items (
    id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind              text        NOT NULL,
    session_id        bigint      REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    operator_turn_id  bigint      REFERENCES memory.operator_turns(id) ON DELETE CASCADE,
    project_id        bigint,
    request_id        bigint,
    origin_session_uuid uuid,
    origin_turn_id    bigint,
    source_ref        text        NOT NULL,
    pattern           text,
    text              text        NOT NULL,
    text_chars        integer     NOT NULL,
    embedding         vector(768) NOT NULL,
    embedder_key      text        NOT NULL REFERENCES memory.semantic_embedders(embedder_key),
    vector_source     text        NOT NULL,
    builder_version   text        NOT NULL,
    effective_at      timestamptz NOT NULL,
    recorded_at       timestamptz NOT NULL DEFAULT now(),
    data              jsonb       NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT semantic_items_kind_check
        CHECK (kind IN ('turn_prompt', 'turn_ending', 'directive', 'anchor')),
    CONSTRAINT semantic_items_vector_source_check
        CHECK (vector_source IN ('chunk_copy', 'embedded', 'member_copy')),
    CONSTRAINT semantic_items_anchor_shape
        CHECK ((kind = 'anchor') = (pattern IS NOT NULL)
               AND (kind <> 'anchor' OR (session_id IS NULL AND operator_turn_id IS NULL
                                         AND origin_session_uuid IS NOT NULL
                                         AND origin_turn_id IS NOT NULL
                                         AND (request_id IS NOT NULL
                                              OR data->>'text_source' = 'operator_quote')))),
    CONSTRAINT semantic_items_turn_shape
        CHECK (kind NOT IN ('turn_prompt', 'turn_ending')
               OR (operator_turn_id IS NOT NULL AND session_id IS NOT NULL)),
    CONSTRAINT semantic_items_text_chars
        CHECK (text_chars = char_length(text) AND text_chars > 0),
    CONSTRAINT semantic_items_source_ref_check
        CHECK (source_ref ~ '^(memory\.operator_turns|memory\.dossier_entries|memory\.pattern_candidates|mirror):[0-9a-z-]+(:[0-9]+)?$'),
    CONSTRAINT uq_semantic_items_key UNIQUE (kind, source_ref, embedder_key, builder_version)
);

COMMENT ON TABLE memory.semantic_items IS
    'JC20. One embedded text per row under one embedder: turn_prompt (chunk 0 of the operator''s genuine message as the chunker cut it; vector copied from operator_turn_chunks chunk 0 only inside the current embedder''s verified copy window), turn_ending (the turn''s last non-sidecar characters, from JC02 text_spans), directive (a JC03 directive span), anchor (an audited mirror instance or an operator-named candidate member). Immutable. As of T: visible when recorded_at <= T (Knowledge mode) or effective_at < T (Reconstructed mode, replays only; JC04 AsOfMode). Eligibility (genuine, operator class) is re-checked against memory.turn_facts at query time, not frozen here.';
COMMENT ON COLUMN memory.semantic_items.text IS
    'Exactly the text that was embedded, whole. Never a preview.';
COMMENT ON COLUMN memory.semantic_items.embedder_key IS
    'The memory.semantic_embedders key the vector was produced under (plain text, no hash). Retrieval reads only the current key; a new key re-embeds into new rows. A chunk_copy item''s source chunk was embedded at or after that embedder''s copy_valid_from.';
COMMENT ON COLUMN memory.semantic_items.origin_session_uuid IS
    'Anchors only: the session of the audited turn (mirror.json confirmed[].session_uuid) or of the candidate member. No FK: anchors outlive their sessions. The tagger excludes anchors whose origin session is the correction''s.';
COMMENT ON COLUMN memory.semantic_items.origin_turn_id IS
    'Anchors only: the audited turn (mirror.json confirmed[].turn_id) or the candidate member''s turn. No FK, for the same reason.';
COMMENT ON COLUMN memory.semantic_items.source_ref IS
    'memory.operator_turns:<id> for turn items, memory.dossier_entries:<id> for directives, mirror:<pattern>:<turn_id> or memory.pattern_candidates:<id>:<item_id> for anchors. Built from ids and slugs, never a hash.';
COMMENT ON COLUMN memory.semantic_items.request_id IS
    'The request that carried the words (prompt request, directive source request, anchor correcting request). No FK: a recording write must not fail on a pruned request (20260829000002_llm_calls.sql reasoning).';
COMMENT ON COLUMN memory.semantic_items.effective_at IS
    'When the operator said the words (the request''s created_at; for a turn_ending, the turn''s closed_at).';

CREATE INDEX IF NOT EXISTS idx_semantic_items_embedding
    ON memory.semantic_items USING hnsw (embedding vector_cosine_ops) WITH (m = 16, ef_construction = 64);
CREATE INDEX IF NOT EXISTS idx_semantic_items_kind_key_recorded
    ON memory.semantic_items (kind, embedder_key, recorded_at);
CREATE INDEX IF NOT EXISTS idx_semantic_items_turn
    ON memory.semantic_items (operator_turn_id, kind) WHERE operator_turn_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_semantic_items_session
    ON memory.semantic_items (session_id, kind) WHERE session_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_semantic_items_anchor_pattern
    ON memory.semantic_items (pattern, embedder_key) WHERE kind = 'anchor';
CREATE INDEX IF NOT EXISTS idx_semantic_items_request
    ON memory.semantic_items (request_id, kind) WHERE request_id IS NOT NULL;

-- Correction qualification joins a correcting turn to its predecessor through
-- JC02's next_genuine_turn_id; JC02 indexes (session_id, operator_turn_id) only.
CREATE INDEX IF NOT EXISTS idx_turn_facts_next_genuine_turn
    ON memory.turn_facts (next_genuine_turn_id) WHERE next_genuine_turn_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS memory.semantic_floors (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    embedder_key     text             NOT NULL,
    pair             text             NOT NULL,
    quantile         double precision NOT NULL CHECK (quantile > 0.5 AND quantile < 1),
    floor            double precision NOT NULL,
    n_pairs          bigint           NOT NULL CHECK (n_pairs > 0),
    p50              double precision NOT NULL,
    p95              double precision NOT NULL,
    p99              double precision NOT NULL,
    p999             double precision NOT NULL,
    sample           jsonb            NOT NULL,
    builder_version  text             NOT NULL,
    status           text             NOT NULL,
    promoted_from_id bigint           REFERENCES memory.semantic_floors(id) ON DELETE SET NULL,
    computed_at      timestamptz      NOT NULL DEFAULT now(),
    CONSTRAINT semantic_floors_pair_check CHECK (pair ~ '^[a-z]+:[a-z]+$'),
    CONSTRAINT semantic_floors_status_check CHECK (status IN ('in_force', 'monitoring', 'insufficient')),
    CONSTRAINT semantic_floors_insufficient CHECK (status <> 'in_force' OR n_pairs >= 10000)
);

COMMENT ON TABLE memory.semantic_floors IS
    'JC20. Chance similarity per (embedder, facet pair): percentiles of cross-session pairs between seeded samples. Only the corr facet reads labels (dev-split qualified corrections, holdout excluded); sample records the tiers read. Append-only. The newest in_force row per (embedder_key, pair, quantile) is in force and its id goes into consumer bundles; in_force rows are written only for a new embedder key or by mike semantic floors promote. The weekly recompute writes monitoring rows. sample holds the seed, the window and the sampled turn ids in full.';

CREATE INDEX IF NOT EXISTS idx_semantic_floors_in_force
    ON memory.semantic_floors (embedder_key, pair, quantile, id DESC) WHERE status = 'in_force';
CREATE INDEX IF NOT EXISTS idx_semantic_floors_monitoring
    ON memory.semantic_floors (embedder_key, pair, quantile, id DESC) WHERE status = 'monitoring';

CREATE TABLE IF NOT EXISTS memory.pattern_candidates (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    status           text        NOT NULL DEFAULT 'open',
    cluster_version  text        NOT NULL,
    embedder_key     text        NOT NULL,
    floor_id         bigint      REFERENCES memory.semantic_floors(id) ON DELETE SET NULL,
    window_start     timestamptz NOT NULL,
    window_end       timestamptz NOT NULL,
    member_item_ids  bigint[]    NOT NULL,
    member_count     integer     NOT NULL,
    session_count    integer     NOT NULL,
    medoid_item_id   bigint,
    created_at       timestamptz NOT NULL DEFAULT now(),
    decided_at       timestamptz,
    decision_note    text,
    named_pattern    text,
    CONSTRAINT pattern_candidates_status_check
        CHECK (status IN ('open', 'named', 'dismissed', 'superseded')),
    CONSTRAINT pattern_candidates_decided
        CHECK ((status IN ('named', 'dismissed')) = (decided_at IS NOT NULL)),
    CONSTRAINT pattern_candidates_named
        CHECK ((status = 'named') = (named_pattern IS NOT NULL)),
    CONSTRAINT pattern_candidates_members
        CHECK (member_count = cardinality(member_item_ids) AND member_count >= 2),
    CONSTRAINT pattern_candidates_window CHECK (window_start < window_end)
);

COMMENT ON TABLE memory.pattern_candidates IS
    'JC20. Weekly mutual-nearest-neighbour groups of corrections no anchor explains. The operator names (a failure_tag row in eval.axis_labels plus one anchor per member) or dismisses, in the SPA only. member_item_ids is the complete list; it has no FK (an array cannot), so a member whose item was removed by session TTL or by prune is reported as gone, never dropped. floor_id is SET NULL when prune removes an old key''s floors, so decided candidates survive as decision records.';

CREATE INDEX IF NOT EXISTS idx_pattern_candidates_status
    ON memory.pattern_candidates (status, created_at DESC);

CREATE TABLE IF NOT EXISTS memory.pattern_assignments (
    id                    bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    corrected_turn_id     bigint           NOT NULL,
    correction_item_id    bigint           NOT NULL REFERENCES memory.semantic_items(id) ON DELETE CASCADE,
    qualification         text             NOT NULL,
    qualification_ref     text             NOT NULL,
    pattern               text,
    score                 double precision NOT NULL,
    similarity            double precision NOT NULL,
    anchor_item_id        bigint           REFERENCES memory.semantic_items(id) ON DELETE SET NULL,
    runner_up_pattern     text,
    margin                double precision,
    top_anchors           jsonb            NOT NULL,
    floor_id              bigint           NOT NULL REFERENCES memory.semantic_floors(id),
    tagger_version        text             NOT NULL,
    embedder_key          text             NOT NULL,
    assigned_at           timestamptz      NOT NULL DEFAULT now(),
    CONSTRAINT pattern_assignments_qualification_check
        CHECK (qualification IN ('gold', 'relayed', 'seed', 'live_verdict', 'session_end')),
    CONSTRAINT pattern_assignments_qualification_ref_check
        CHECK (qualification_ref ~ '^(eval\.labels|memory\.turn_verdicts|session_judges\.verdicts):[0-9]+$'),
    CONSTRAINT uq_pattern_assignments UNIQUE (correction_item_id, tagger_version, embedder_key)
);

COMMENT ON TABLE memory.pattern_assignments IS
    'JC20. Nearest-anchor pattern for a qualified correction (Internal qualification: gold, relayed, seed dev labels, live verdicts, session ends). Anchors from the correction''s own session, or built from its own correcting request, are never candidates. pattern is NULL (unassigned) when the best score is below semantic.tagger_min_score. A derived feature, never a label: nothing here is written to eval.labels. top_anchors holds the three nearest anchors whole (item id, pattern, similarity, score).';
COMMENT ON COLUMN memory.pattern_assignments.corrected_turn_id IS
    'The turn the correction was about. No FK: the row goes with its correction item (ON DELETE CASCADE), and a reader treats a missing turn as gone.';

CREATE INDEX IF NOT EXISTS idx_pattern_assignments_pattern
    ON memory.pattern_assignments (pattern, assigned_at DESC) WHERE pattern IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_pattern_assignments_unassigned
    ON memory.pattern_assignments (assigned_at DESC) WHERE pattern IS NULL;
CREATE INDEX IF NOT EXISTS idx_pattern_assignments_turn
    ON memory.pattern_assignments (corrected_turn_id);

CREATE SCHEMA IF NOT EXISTS eval;

CREATE TABLE IF NOT EXISTS eval.embedding_drift (
    id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    day            date             NOT NULL,
    stream         text             NOT NULL CHECK (stream IN ('prompt', 'ending', 'correction')),
    embedder_key   text             NOT NULL,
    drift_version  text             NOT NULL,
    n_current      integer          NOT NULL CHECK (n_current >= 0),
    n_baseline     integer          NOT NULL CHECK (n_baseline >= 0),
    distance       double precision,
    z              double precision,
    prior_days     integer          NOT NULL CHECK (prior_days >= 0),
    prior_mean     double precision,
    prior_sd       double precision,
    status         text             NOT NULL CHECK (status IN ('scored', 'warming', 'too_few_items')),
    computed_at    timestamptz      NOT NULL DEFAULT now(),
    CONSTRAINT uq_embedding_drift_day UNIQUE (day, stream, embedder_key, drift_version),
    CONSTRAINT embedding_drift_scored_has_distance CHECK ((status = 'scored') = (distance IS NOT NULL))
);

COMMENT ON TABLE eval.embedding_drift IS
    'JC20. Daily 1 - cosine between the centroid of the trailing 7 days and of the 28 days before, per stream, with z against the previous 28 daily distances. Feeds the embedding_drift alarm and the inputs block of label_mix_shift.';

-- ── eval.alarms: the embedding_drift kind ────────────────────────────
-- The kind CHECK is a closed list; this re-creates it with every kind JC08
-- shipped, the kinds other packages have reserved, and embedding_drift. Never
-- drop a kind: a package that re-creates this CHECK later merges this list.
ALTER TABLE eval.alarms DROP CONSTRAINT IF EXISTS alarms_kind_check;
ALTER TABLE eval.alarms
    ADD CONSTRAINT alarms_kind_check
    CHECK (kind IN ('label_mix_shift', 'unjudged_backlog', 'judge_stalled',
                    'budget_refusals', 'change_without_gate', 'gold_label_by_agent',
                    'steer_question_eligible', 'steer_question_demoted',
                    'steer_holdback_harm', 'steer_wrong_wake_ceiling',
                    'metric_drop', 'shadow_disagreement',
                    'judge_call_loop',
                    'embedding_drift'));

-- ── config.feature_toggles: semantic_context ─────────────────────────
-- The key CHECK is re-created in full from the latest list (dossier_builder,
-- JC02) plus semantic_context. The row starts on and operator_set false: the
-- worker changes no Claude traffic and every embed call checks that the
-- embedding URL is loopback, so no text leaves the machine unless
-- semantic.allow_remote_embedding is set.
ALTER TABLE config.feature_toggles DROP CONSTRAINT IF EXISTS feature_toggles_key_check;
ALTER TABLE config.feature_toggles
    ADD CONSTRAINT feature_toggles_key_check
    CHECK (key IN ('reminders', 'recall_injector', 'weasel_intercept', 'steerer', 'lesson_worker',
                   'harvester', 'db_resync', 'judge_worker', 'session_judge', 'reminder_on_token',
                   'mailbox_redelivery', 'agent_lessons', 'file_read_compression', 'retention_sweep',
                   'proxy_tools', 'channel_link_sweep', 'workspace_capture_registration',
                   'self_identity_reminder', 'workspace_briefing', 'prompt_assistant',
                   'dossier_builder', 'semantic_context'));

INSERT INTO config.feature_toggles (key, enabled, operator_set) VALUES
    ('semantic_context', true, false)
ON CONFLICT (key) DO NOTHING;
