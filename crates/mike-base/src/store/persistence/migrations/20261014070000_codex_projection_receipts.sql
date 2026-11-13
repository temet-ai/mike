CREATE UNIQUE INDEX idx_runtime_events_namespace_id ON runtime.events (namespace_id, id);

CREATE TABLE runtime.codex_intake_state (
    namespace_id uuid PRIMARY KEY REFERENCES runtime.namespaces(id),
    last_capture_sequence bigint NOT NULL DEFAULT 0 CHECK (last_capture_sequence >= 0)
);

CREATE TABLE runtime.codex_receipt_context (
    event_id bigint PRIMARY KEY,
    namespace_id uuid NOT NULL,
    capture_sequence bigint NOT NULL CHECK (capture_sequence > 0),
    context_version integer NOT NULL CHECK (context_version > 0),
    connection_epoch uuid NOT NULL CHECK (connection_epoch <> '00000000-0000-0000-0000-000000000000'::uuid),
    frame_ordinal bigint NOT NULL CHECK (frame_ordinal >= 0),
    transport_gap boolean NOT NULL,
    context_document json NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (namespace_id, event_id) REFERENCES runtime.events(namespace_id, id),
    UNIQUE (namespace_id, capture_sequence),
    UNIQUE (namespace_id, connection_epoch, frame_ordinal),
    UNIQUE (namespace_id, event_id, capture_sequence)
);

CREATE TABLE runtime.codex_projection_generations (
    generation_id uuid PRIMARY KEY CHECK (generation_id <> '00000000-0000-0000-0000-000000000000'::uuid),
    namespace_id uuid NOT NULL REFERENCES runtime.namespaces(id),
    projector_version integer NOT NULL CHECK (projector_version > 0),
    state_codec_version integer NOT NULL CHECK (state_codec_version > 0),
    state text NOT NULL CHECK (state IN ('building', 'active', 'retired', 'failed')),
    last_applied_capture_sequence bigint NOT NULL DEFAULT 0 CHECK (last_applied_capture_sequence >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    activated_at timestamptz,
    UNIQUE (namespace_id, generation_id)
);

CREATE TABLE runtime.codex_projection_heads (
    namespace_id uuid PRIMARY KEY REFERENCES runtime.namespaces(id),
    active_generation_id uuid,
    FOREIGN KEY (namespace_id, active_generation_id)
        REFERENCES runtime.codex_projection_generations(namespace_id, generation_id)
);

CREATE TABLE runtime.codex_projection_applications (
    generation_id uuid NOT NULL,
    namespace_id uuid NOT NULL,
    event_id bigint NOT NULL,
    capture_sequence bigint NOT NULL CHECK (capture_sequence > 0),
    status text NOT NULL CHECK (status IN ('projected', 'partial', 'unsupported', 'malformed', 'excluded')),
    issues json NOT NULL,
    effect_count integer NOT NULL CHECK (effect_count >= 0),
    applied_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (generation_id, event_id),
    UNIQUE (generation_id, capture_sequence),
    FOREIGN KEY (namespace_id, generation_id)
        REFERENCES runtime.codex_projection_generations(namespace_id, generation_id),
    FOREIGN KEY (namespace_id, event_id, capture_sequence)
        REFERENCES runtime.codex_receipt_context(namespace_id, event_id, capture_sequence)
);

CREATE TABLE runtime.codex_projection_effects (
    generation_id uuid NOT NULL,
    namespace_id uuid NOT NULL,
    event_id bigint NOT NULL,
    effect_ordinal integer NOT NULL CHECK (effect_ordinal >= 0),
    capture_sequence bigint NOT NULL,
    effect_kind text NOT NULL CHECK (effect_kind IN ('item', 'lifecycle', 'signal', 'usage', 'billing')),
    thread_id text NOT NULL CHECK (btrim(thread_id) <> ''),
    turn_id text CHECK (turn_id IS NULL OR btrim(turn_id) <> ''),
    item_id text CHECK (item_id IS NULL OR btrim(item_id) <> ''),
    observation json NOT NULL,
    PRIMARY KEY (generation_id, event_id, effect_ordinal),
    FOREIGN KEY (generation_id, event_id)
        REFERENCES runtime.codex_projection_applications(generation_id, event_id)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (namespace_id, generation_id)
        REFERENCES runtime.codex_projection_generations(namespace_id, generation_id),
    FOREIGN KEY (namespace_id, event_id, capture_sequence)
        REFERENCES runtime.codex_receipt_context(namespace_id, event_id, capture_sequence)
);

CREATE INDEX idx_codex_effects_thread_order ON runtime.codex_projection_effects
    (generation_id, thread_id, capture_sequence, effect_ordinal);
CREATE INDEX idx_codex_effects_item_order ON runtime.codex_projection_effects
    (generation_id, thread_id, turn_id, item_id, capture_sequence, effect_ordinal)
    WHERE item_id IS NOT NULL;
CREATE INDEX idx_codex_effects_kind_order ON runtime.codex_projection_effects
    (generation_id, thread_id, effect_kind, capture_sequence, effect_ordinal);

CREATE TABLE runtime.codex_item_state (
    generation_id uuid NOT NULL,
    namespace_id uuid NOT NULL,
    thread_id text NOT NULL CHECK (btrim(thread_id) <> ''),
    turn_id text NOT NULL CHECK (btrim(turn_id) <> ''),
    item_id text NOT NULL CHECK (btrim(item_id) <> ''),
    terminal boolean NOT NULL,
    reconciliation_required boolean NOT NULL,
    first_capture_sequence bigint NOT NULL CHECK (first_capture_sequence > 0),
    latest_capture_sequence bigint NOT NULL CHECK (latest_capture_sequence >= first_capture_sequence),
    state_codec_version integer NOT NULL CHECK (state_codec_version > 0),
    state_document json NOT NULL,
    PRIMARY KEY (generation_id, thread_id, turn_id, item_id),
    FOREIGN KEY (namespace_id, generation_id)
        REFERENCES runtime.codex_projection_generations(namespace_id, generation_id)
);

CREATE INDEX idx_codex_items_stable_order ON runtime.codex_item_state
    (generation_id, thread_id, first_capture_sequence, turn_id, item_id);
