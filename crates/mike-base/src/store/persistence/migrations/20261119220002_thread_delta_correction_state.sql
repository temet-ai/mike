-- Progress of the thread-delta correction job (#354). Rows captured before 2026-11-17 carry no
-- predecessor_rule_id: their predecessor was the lane guess, and a continue request recorded the
-- whole earlier conversation as removed. The job walks those requests in id order, takes the
-- request whose response the thread link names as predecessor, records the rule, and deletes the
-- removed rows the link proves are noise (continue and create requests, agent label calls). One
-- row; the job holds an advisory lock per batch, so one proxy runs it although every machine
-- shares this database.

CREATE TABLE IF NOT EXISTS requests.delta_correction_state (
    id smallint PRIMARY KEY CHECK (id = 1),
    last_request_id bigint NOT NULL DEFAULT 0,
    requests_corrected bigint NOT NULL DEFAULT 0,
    removed_rows_deleted bigint NOT NULL DEFAULT 0,
    finished_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO requests.delta_correction_state (id) VALUES (1) ON CONFLICT (id) DO NOTHING;
