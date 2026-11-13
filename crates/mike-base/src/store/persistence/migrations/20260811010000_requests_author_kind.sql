-- Adds the author-kind classifier that #104 and #108 both wait on (their
-- own plan comments name #105 as upstream).
--
-- Design: follows requests.requests's own sibling-column pattern already
-- established at baseline (turn_origin_id / turn_type_id / status_id /
-- method_id) -- a tiny lookup table + NOT NULL FK column on the same table,
-- not a satellite fact table like prompt_kinds.classifications. That one is
-- deliberately backfillable/per-operator-turn; author_kind must be known at
-- the same INSERT that writes the request row (same grain as turn_origin_id),
-- so a satellite table would just reintroduce the join-and-hope-it-exists
-- gap #105 exists to close.
--
-- Two-phase FK add: ADD COLUMN with a constant DEFAULT is metadata-only in
-- PG11+ (no table rewrite) on this 240k+-row write-heavy table; the FK
-- constraint is added NOT VALID then validated in a separate statement so
-- the validation scan doesn't hold the same lock tier as the initial ADD
-- CONSTRAINT would -- matters because the real deploy path (the alt-port
-- smoke test) applies tree migrations to the live DB while the outgoing
-- proxy instance is still writing requests.requests.
--
-- No backfill: historical rows keep the DEFAULT 'unknown' (id=4) per the
-- issue's explicit scope ("Backfill is out of scope for this issue").

-- Every statement is re-runnable: this migration's effects can already be
-- present on a database that applied them without recording the version row,
-- and a non-idempotent statement then fails every subsequent boot.

CREATE TABLE IF NOT EXISTS requests.author_kinds (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text   NOT NULL UNIQUE
);

INSERT INTO requests.author_kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'operator'),
    (2, 'harness'),
    (3, 'agent'),
    (4, 'unknown')
ON CONFLICT DO NOTHING;

ALTER TABLE requests.requests
    ADD COLUMN IF NOT EXISTS author_kind_id bigint NOT NULL DEFAULT 4;

-- Postgres has no ADD CONSTRAINT IF NOT EXISTS, hence the catalogue guard.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'requests_author_kind_id_fkey'
    ) THEN
        ALTER TABLE requests.requests
            ADD CONSTRAINT requests_author_kind_id_fkey
                FOREIGN KEY (author_kind_id) REFERENCES requests.author_kinds(id)
                NOT VALID;
    END IF;

    IF EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conname = 'requests_author_kind_id_fkey' AND NOT convalidated
    ) THEN
        ALTER TABLE requests.requests
            VALIDATE CONSTRAINT requests_author_kind_id_fkey;
    END IF;
END
$$;

-- Index every FK column (project convention, mandatory) -- also backs the
-- read paths #105 exists to unblock (#104, #108): filtering requests by
-- author_kind instead of walking the subagent_persona/agent_instance_id
-- heuristics that only populate on 17%/5% of live rows.
CREATE INDEX IF NOT EXISTS idx_requests_author_kind_id
    ON requests.requests USING btree (author_kind_id);
