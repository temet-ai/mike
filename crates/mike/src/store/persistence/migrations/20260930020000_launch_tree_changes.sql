CREATE TABLE launch.tree_changes (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    run_id      TEXT NOT NULL,
    path        TEXT NOT NULL,
    change      TEXT NOT NULL CHECK (change IN ('added', 'modified', 'deleted')),
    content_sha TEXT,
    seen_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE launch.tree_changes IS 'What changed in a fleet run''s checkout, read from the tree itself once a second while the run is running: one row per path per observed change, with the blob hash of the new content. Who made the change is worked out at read time from the seats'' tool calls in flight at seen_at.';

CREATE INDEX idx_tree_changes_run_seen ON launch.tree_changes (run_id, seen_at, id);
CREATE INDEX idx_tree_changes_run_path ON launch.tree_changes (run_id, path, seen_at);
