CREATE TABLE launch.told (
    run_id  TEXT NOT NULL,
    what    TEXT NOT NULL,
    told_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, what)
);

COMMENT ON TABLE launch.told IS 'mike''s once-only ledger for the mail it sends a fleet run by itself: one row per thing already said in a run (what is the notice key). The tick reads it before it mails, so a restart does not send the same notice again.';
