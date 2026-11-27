-- The knowledge sweep reconciles every knowledge kind with its source once a day
-- (keep_items_current in crates/mike-knowledge/src/embed_worker/knowledge.rs, RECONCILE_EVERY).
-- It kept the time of the last reconcile only in memory, so every restart and every move of the
-- knowledge sweep lease to another machine ran it again: about 8 runs a day of ~0.2 s per kind
-- over 11 kinds (mike#459, problem 37116). memory.reconcile_knowledge_items now records the time
-- here, and a new process or a new lease holder reads it before it reconciles.
CREATE TABLE IF NOT EXISTS memory.knowledge_upkeep (
    id            SMALLINT    PRIMARY KEY CHECK (id = 1),
    reconciled_at TIMESTAMPTZ NOT NULL
);

COMMENT ON TABLE memory.knowledge_upkeep IS
    'One row: when the knowledge sweep last reconciled every knowledge kind with its source, on any machine. The sweep skips the daily reconcile while this is under 24 hours old.';
