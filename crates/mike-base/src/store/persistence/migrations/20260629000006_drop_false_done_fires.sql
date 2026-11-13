-- Drop steering.false_done_fires — all 60 historical rows migrated into
-- steering.interventions by migration 20260629000002. All reads (false_done_rate_v,
-- fetch_false_done_counts_for_session) and writes (insert_false_done_fire, backfill)
-- were removed in U3 and now operate on steering.interventions.
-- Indexes drop automatically with the table.

DROP TABLE IF EXISTS steering.false_done_fires;
