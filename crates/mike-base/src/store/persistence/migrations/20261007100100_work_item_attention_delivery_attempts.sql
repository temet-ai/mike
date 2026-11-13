ALTER TABLE agents.work_item_attention
    ADD COLUMN IF NOT EXISTS delivery_attempts integer NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_delivery_attempt_at timestamptz;
