CREATE TABLE launch.introductions (
    again_use_id BIGINT PRIMARY KEY,
    run_id       TEXT NOT NULL,
    first_use_id BIGINT NOT NULL,
    told_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE launch.introductions IS 'One row per slow command a fleet seat ran after another seat of the same run had already run the same command: again_use_id is the repeat (tools.uses.id), first_use_id the earlier run. mike mails the coordinator once per row so it can put the two seats in touch; the row is what keeps that to once.';

CREATE INDEX idx_introductions_run ON launch.introductions (run_id, told_at);
