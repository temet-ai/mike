-- wright.log_lines: the ERROR and WARN lines each machine reads from its own logs (the proxy's
-- JSON log, the forward sidecar's log, panics in the proxy's stdout) over the last hour, so the
-- leased log_errors and log_warnings checks see every machine and every process, not one file on
-- one host. Each machine inserts its own new lines every pass and deletes its own rows older
-- than two hours.

SET LOCAL lock_timeout = '10s';

CREATE TABLE wright.log_lines (
    id           bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    machine_uuid uuid        NOT NULL,
    source       text        NOT NULL,
    at           timestamptz NOT NULL,
    level        text        NOT NULL,
    target       text        NOT NULL,
    message      text        NOT NULL,
    raw          text        NOT NULL,
    collected_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_wright_log_lines_at ON wright.log_lines (at);
CREATE INDEX idx_wright_log_lines_machine_source_at ON wright.log_lines (machine_uuid, source, at);

COMMENT ON TABLE wright.log_lines IS
    'ERROR and WARN log lines of the last two hours, one row per line, collected by every machine from its own logs; read by the log_errors and log_warnings checks.';

UPDATE wright.checks
   SET params = jsonb_set(params, '{repeats}', '30'),
       expects = 'No WARN log line of one target and shape repeats 30 times within an hour',
       updated_at = now()
 WHERE name = 'log_warnings';
