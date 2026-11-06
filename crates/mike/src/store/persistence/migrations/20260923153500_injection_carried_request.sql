ALTER TABLE remote_control.injections ADD COLUMN carried_request_id bigint;

CREATE INDEX idx_rc_injections_uncarried
    ON remote_control.injections (channel_id)
 WHERE acked_at IS NOT NULL AND carried_request_id IS NULL;
