-- Preserve immutable proposer inputs and wire responses before optional embedding.
-- JSONB holds replay DTOs, not filter keys: lifecycle and budget fields are typed.
CREATE TABLE IF NOT EXISTS memory.lesson_proposal_jobs (
    verdict_id bigint PRIMARY KEY REFERENCES memory.turn_verdicts(id) ON DELETE CASCADE,
    candidate jsonb NOT NULL,
    proposed jsonb NOT NULL,
    "similar" jsonb NOT NULL,
    model text NOT NULL,
    cost_usd double precision,
    generation_id text,
    retry_count integer NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    next_attempt_at timestamptz NOT NULL DEFAULT now(),
    status text NOT NULL CHECK (status IN ('awaiting_embedding', 'rejected', 'applied')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
COMMENT ON TABLE memory.lesson_proposal_jobs IS
    'One durable replay job per verdict; nullable cost and generation ID reflect absent provider usage metadata.';
CREATE INDEX IF NOT EXISTS idx_lesson_proposal_jobs_pending
    ON memory.lesson_proposal_jobs (updated_at, verdict_id)
    WHERE status = 'awaiting_embedding';
CREATE TABLE IF NOT EXISTS memory.lesson_proposer_daily_budget (
    day date PRIMARY KEY,
    consumed bigint NOT NULL CHECK (consumed >= 0)
);
-- A deployment must not reset spend already recorded by this worker.
INSERT INTO memory.lesson_proposer_daily_budget (day, consumed)
SELECT (created_at AT TIME ZONE 'UTC')::date, count(*)
FROM llm.calls WHERE role = 'lesson_proposer'
GROUP BY (created_at AT TIME ZONE 'UTC')::date
ON CONFLICT (day) DO UPDATE
SET consumed = greatest(memory.lesson_proposer_daily_budget.consumed, EXCLUDED.consumed);

-- Immutable original evidence is retained while ambiguous dependency failures
-- are reassessed once under the normal daily budget. No claim that every row
-- represented a valid rule: the historical writer did not retain that field.
CREATE TABLE IF NOT EXISTS memory.lesson_proposal_recovery (
    verdict_id bigint PRIMARY KEY REFERENCES memory.turn_verdicts(id) ON DELETE CASCADE,
    original_evidence jsonb NOT NULL,
    recovery_state text NOT NULL DEFAULT 'needs_reproposal'
        CHECK (recovery_state IN ('needs_reproposal', 'reproposed', 'obsolete')),
    disposition_reason text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now()
);
COMMENT ON COLUMN memory.lesson_proposal_recovery.original_evidence IS
    'Full immutable historical evidence row, kept as an audit payload before the current row is replaced.';
INSERT INTO memory.lesson_proposal_recovery (verdict_id, original_evidence)
SELECT e.verdict_id, to_jsonb(e) FROM memory.lesson_proposal_evidence e
WHERE e.lesson_id IS NULL AND e.model <> 'none'
  AND e.created_at >= '2026-10-01T13:02:13.730511Z'::timestamptz
  AND e.created_at < now()
ON CONFLICT (verdict_id) DO NOTHING;

-- Shared eligibility for initial selection and replay/legacy reconciliation.
CREATE OR REPLACE VIEW memory.lesson_eligible_corrections_v AS
SELECT v.id AS verdict_id, v.operator_turn_id, v.session_id, s.project_id,
       v.confidence, v.rationale, v.unmet_ask, v.created_at,
       ot.user_prompt, ot.final_assistant_text, r.human_prompt AS correcting_message
FROM memory.turn_verdicts v
JOIN sessions.sessions s ON s.id = v.session_id
JOIN memory.operator_turns ot ON ot.id = v.operator_turn_id
JOIN memory.turn_facts tf ON tf.operator_turn_id = v.operator_turn_id
JOIN requests.requests r ON r.id = tf.next_genuine_request_id
LEFT JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
LEFT JOIN prompt_kinds.classifications pc ON pc.request_id = r.id
LEFT JOIN prompt_kinds.kinds pk ON pk.id = pc.kind_id
WHERE v.mode = 'live' AND v.reaction = 'corrected'
  AND v.projected AND ot.live_verdict_id = v.id
  AND v.next_genuine_request_id = tf.next_genuine_request_id
  AND tf.prompt_kind = 'genuine' AND tf.next_state = 'resolved'
  AND ak.code IN ('operator', 'unknown')
  AND (pk.code = 'operator' OR (ak.code = 'operator' AND pk.code IS NULL));
