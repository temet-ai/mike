-- `proxy.row_changed` for mailbox.messages also fires when a message becomes
-- claimable again without any other column moving: its hold is lifted
-- (held_until_turn true -> false, `release_held_mail`) or its claim is
-- released (claimed_at set -> NULL, `unclaim_message`).
--
-- The channel claim and the dead-letter revive remember per recipient that
-- they found nothing, until the recipient's mailbox generation moves. Those
-- two transitions were silent, so another machine's memo would have kept
-- answering "nothing" for a message that had become claimable. Setting
-- claimed_at (every claim) stays silent: it only makes a message less
-- claimable.

CREATE OR REPLACE TRIGGER trg_mailbox_messages_row_changed_update
AFTER UPDATE ON mailbox.messages
FOR EACH ROW
WHEN ((OLD.sent_at, OLD.consumed_at, OLD.dead_lettered_at, OLD.delivery_attempts,
       OLD.channel_delivered_at, OLD.sent_request_id, OLD.thread_id, OLD.in_reply_to_id,
       OLD.to_session_id, OLD.from_session_id, OLD.queued_at, OLD.body,
       OLD.held_until_turn)
      IS DISTINCT FROM
      (NEW.sent_at, NEW.consumed_at, NEW.dead_lettered_at, NEW.delivery_attempts,
       NEW.channel_delivered_at, NEW.sent_request_id, NEW.thread_id, NEW.in_reply_to_id,
       NEW.to_session_id, NEW.from_session_id, NEW.queued_at, NEW.body,
       NEW.held_until_turn)
      OR (OLD.claimed_at IS NOT NULL AND NEW.claimed_at IS NULL))
EXECUTE FUNCTION mike.notify_row_changed_mailbox_messages();
