-- no-transaction
-- guard_label_candidates finds the newest admitted pass of each (turn, question, trigger) for
-- every turn with an unlabelled candidate: about 15k rows across 6.4k heap pages for steerer 18.
-- This index carries every column that lookup reads, so it is answered from the index.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_evaluations_admitted_turn
    ON steering.evaluations (operator_turn_id)
    INCLUDE (steerer_id, question, trigger_point, dispatched_at, id)
    WHERE status <> 'dispatched' AND (mode = 'shadow' OR arm IN ('holdback', 'precheck_held'));
