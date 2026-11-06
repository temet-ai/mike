-- JC11: `mike eval import holdout --resplit-unused-dev` moves a sampled turn from the dev split to
-- holdout when no eval run has used it. The fleet-mirror import put every labelled turn into dev
-- without a holdout share, so a clean holdout sample drawn later lands mostly on dev turns whose
-- single-model labels were never read by any run. The move is recorded on the turn with its own
-- split_reason, so it stays distinguishable from a turn imported straight into holdout.
--
-- Expand-only: the CHECK gains one value.

ALTER TABLE eval.turn_splits DROP CONSTRAINT IF EXISTS turn_splits_split_reason_check;
ALTER TABLE eval.turn_splits
    ADD CONSTRAINT turn_splits_split_reason_check
        CHECK (split_reason IN ('seed_import', 'holdout_import', 'hash', 'holdout_resplit_unused_dev'));
