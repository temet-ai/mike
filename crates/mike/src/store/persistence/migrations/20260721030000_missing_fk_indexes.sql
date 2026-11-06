-- Index the five foreign-key columns that reference requests.requests without one.
--
-- Every one of these FKs is ON DELETE SET NULL or CASCADE, so deleting a parent
-- row fires a per-row referential-integrity trigger that runs
-- `... WHERE <fk_col> = $1` against the child. With no index on the child column
-- that is a sequential scan of the child table per deleted parent row. On
-- requests.deltas (46 GB) that made a 14k-request delete unrunnable: two attempts
-- were abandoned after 8 and 10 minutes without finishing the cascade. Adding
-- idx_deltas_predecessor_request_id (69 MB) is what made the delete complete.
--
-- Retention and project-removal work both delete parents in bulk, so these
-- indexes are load-bearing for any prune, not just the one that surfaced them.
--
-- Partial WHERE ... IS NOT NULL on the four nullable columns: the RI trigger only
-- ever probes for a concrete id, so NULL rows are dead weight in the index.
-- requests.system_prompts.request_id is NOT NULL, so it gets a plain index.
--
-- IF NOT EXISTS: idempotent, so an environment where these were applied by hand
-- converges with a fresh migrate.

CREATE INDEX IF NOT EXISTS idx_deltas_predecessor_request_id
    ON requests.deltas (predecessor_request_id)
    WHERE predecessor_request_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_requests_predecessor_request_id
    ON requests.requests (predecessor_request_id)
    WHERE predecessor_request_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_system_prompts_request_id
    ON requests.system_prompts (request_id);

CREATE INDEX IF NOT EXISTS idx_lessons_request_id
    ON memory.lessons (request_id)
    WHERE request_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_agent_instances_first_request_id
    ON agents.instances (first_request_id)
    WHERE first_request_id IS NOT NULL;

-- The remaining unindexed FKs whose parent is a real data table, so their
-- delete-side triggers actually fire during a project purge:
-- tools.uses (151k rows) → agents.dispatches (991) → agents.instances, and
-- memory.operator_turns (8.2k) ← memory.lessons.

CREATE INDEX IF NOT EXISTS idx_agent_instances_dispatch_id
    ON agents.instances (dispatch_id)
    WHERE dispatch_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_agent_dispatches_tool_use_id
    ON agents.dispatches (tool_use_id)
    WHERE tool_use_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_lessons_source_turn_id
    ON memory.lessons (source_turn_id)
    WHERE source_turn_id IS NOT NULL;

-- Deliberately NOT indexed: the 16 remaining unindexed FK columns all point at
-- static lookup tables — requests.turn_types (3 rows), requests.methods (9),
-- requests.delta_change_kinds (2), messages.roles (3), responses.statuses (71),
-- memory.chunk_roles (2), and the lesson/rule/override enum tables. Their rows
-- are never deleted, so the referential-integrity trigger never fires on the
-- delete path that motivates this migration. Indexing requests.turn_type_id or
-- requests.method_id would add GB-scale indexes to a 103 GB table for no gain.
