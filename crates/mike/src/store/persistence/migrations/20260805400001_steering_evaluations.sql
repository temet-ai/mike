-- Issue #117: visible validation state ("being validated" indicator +
-- verdict attach). `steering.interventions` records only FIRES — a "pass"
-- verdict (the common case) is recorded nowhere today, so the SPA has no
-- signal to show while a completion-claim is being checked or once it
-- clears clean. `steering.evaluations` is a SEPARATE, additive table
-- recording every false_done evaluation ATTEMPT (dispatched -> resolved),
-- regardless of outcome. `steering.interventions`'s fire-semantics are
-- unchanged by this migration — no columns added, no rows implied there.

CREATE TABLE steering.evaluations (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uuid             uuid DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    steerer_id       bigint NOT NULL
                       REFERENCES steering.steerers(id) ON DELETE RESTRICT,
    session_id       bigint NOT NULL
                       REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    operator_turn_id bigint NOT NULL
                       REFERENCES memory.operator_turns(id) ON DELETE CASCADE,
    status           text NOT NULL
                       CHECK (status IN ('dispatched', 'resolved_pass', 'resolved_correction')),
    -- Set only when status = 'resolved_correction' — the intervention this
    -- evaluation's fire produced. FK to interventions.uuid (UNIQUE, not the
    -- PK) so the caller can resolve this row with the same uuid
    -- `judge_turn_close_now` already returns, no extra id lookup needed.
    intervention_uuid uuid
                       REFERENCES steering.interventions(uuid) ON DELETE SET NULL,
    dispatched_at    timestamptz NOT NULL DEFAULT now(),
    resolved_at      timestamptz
);

CREATE INDEX idx_evaluations_steerer_id ON steering.evaluations (steerer_id);
CREATE INDEX idx_evaluations_operator_turn_id ON steering.evaluations (operator_turn_id);
-- Hot path for issue #117's REST route: "latest evaluation for this
-- session", ordered by recency.
CREATE INDEX idx_evaluations_session_dispatched
    ON steering.evaluations (session_id, dispatched_at DESC);
CREATE INDEX idx_evaluations_intervention_uuid
    ON steering.evaluations (intervention_uuid)
    WHERE intervention_uuid IS NOT NULL;

COMMENT ON TABLE steering.evaluations IS
    'Every false_done evaluation attempt (dispatched -> resolved_pass / '
    'resolved_correction), independent of steering.interventions which '
    'records fires only. Backs issue #117''s visible validation state.';
