CREATE TABLE launch.tree_checks (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    run_id      TEXT NOT NULL,
    state       TEXT NOT NULL,
    command     TEXT NOT NULL,
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    exit_code   INTEGER,
    output      TEXT NOT NULL DEFAULT '',
    askers      TEXT[] NOT NULL DEFAULT '{}',
    told        TEXT[] NOT NULL DEFAULT '{}'
);

COMMENT ON TABLE launch.tree_checks IS 'The project''s declared check command, run by mike for a fleet run once per state of the tree: `state` names the content of every file under the check''s declared paths, so a second seat asking at the same state gets this row instead of a second run. `askers` are the seats that asked, `told` the seats mike mailed because their file failed it. The whole output is kept.';

CREATE INDEX idx_tree_checks_run_state ON launch.tree_checks (run_id, state, id DESC);
