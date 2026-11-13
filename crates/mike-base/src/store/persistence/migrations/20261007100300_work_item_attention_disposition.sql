ALTER TABLE agents.work_item_attention
    ADD COLUMN IF NOT EXISTS disposition text NOT NULL DEFAULT 'pending'
        CHECK (disposition IN ('pending', 'delivered', 'stale', 'resolved', 'given_up')),
    ADD COLUMN IF NOT EXISTS disposition_reason text,
    ADD COLUMN IF NOT EXISTS disposition_at timestamptz;

UPDATE agents.work_item_attention
   SET disposition = 'delivered',
       disposition_reason = 'mailed',
       disposition_at = now()
 WHERE delivered_message_uuid IS NOT NULL
   AND disposition = 'pending';
