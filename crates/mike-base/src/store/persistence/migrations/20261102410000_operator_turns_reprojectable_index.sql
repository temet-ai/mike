-- no-transaction
-- reproject_final_turns only revisits turns with no outcome source or a judge-sourced one; the window index alone made it read every closed turn of the live window.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_operator_turns_reprojectable ON memory.operator_turns (closed_at) WHERE closed_at IS NOT NULL AND (outcome_source IS NULL OR outcome_source IN ('judge', 'frustration_close'));
