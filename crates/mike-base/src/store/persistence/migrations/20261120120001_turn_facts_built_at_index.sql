-- no-transaction
-- The seat-memory refresh recomputes a seat session when one of its turn facts was built since
-- the previous refresh: a seat report enters memory.seat_memory only once its turn has a
-- fleet_seat turn fact, and most turn facts are built well after the turn closes.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_turn_facts_built_at ON memory.turn_facts (built_at) INCLUDE (session_id);
