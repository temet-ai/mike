#[cfg(test)]
pub(super) const ENQUEUE: &str = "WITH row AS (
         INSERT INTO mailbox.outbox
             (message_uuid, to_session, from_session, runtime, scope, body, generation)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (message_uuid) DO NOTHING
         RETURNING message_uuid
     ), staged AS (
         INSERT INTO mailbox.outbox_stages (message_uuid, stage, evidence)
         SELECT message_uuid, 'queued', 'written to the outbox' FROM row
         RETURNING 1
     )
     SELECT EXISTS (SELECT 1 FROM staged)";

pub(super) const LOAD: &str = "SELECT message_uuid, to_session, from_session, runtime, scope,
        body, generation, status
   FROM mailbox.outbox WHERE message_uuid = $1";

pub(super) const LATEST_ATTEMPT: &str = "SELECT attempt_uuid AS attempt, attempt_no AS number,
        generation, mode, expected_turn_id, outcome, turn_id, detail
   FROM mailbox.outbox_attempts
  WHERE message_uuid = $1 ORDER BY attempt_no DESC LIMIT 1";

#[cfg(test)]
pub(super) const ALL_ATTEMPTS: &str = "SELECT attempt_uuid AS attempt, attempt_no AS number,
        generation, mode, expected_turn_id, outcome, turn_id, detail
   FROM mailbox.outbox_attempts
  WHERE message_uuid = $1 ORDER BY attempt_no";

pub(super) const REBIND: &str = "UPDATE mailbox.outbox SET generation = $2, updated_at = now()
  WHERE message_uuid = $1 AND generation IS DISTINCT FROM $2";

pub(super) const BEGIN_ATTEMPT: &str = "INSERT INTO mailbox.outbox_attempts
     (attempt_uuid, message_uuid, attempt_no, generation, mode, expected_turn_id)
 SELECT $1, $2, COALESCE(max(attempt_no), 0) + 1, $3, $4, $5
   FROM mailbox.outbox_attempts WHERE message_uuid = $2";

pub(super) const FINISH_ATTEMPT: &str = "UPDATE mailbox.outbox_attempts
    SET outcome = $2, turn_id = $3, detail = $4, finished_at = now()
  WHERE attempt_uuid = $1 AND outcome IS NULL";

pub(super) const RECORD_STAGE: &str = "WITH latest AS (
         SELECT max(attempt_no) AS attempt_no FROM mailbox.outbox_attempts
          WHERE message_uuid = $1
     ), fence AS (
         SELECT $3::uuid IS NULL OR EXISTS (
             SELECT 1
               FROM mailbox.outbox_attempts a
               JOIN mailbox.outbox o ON o.message_uuid = a.message_uuid
               JOIN latest ON latest.attempt_no = a.attempt_no
              WHERE a.attempt_uuid = $3
                AND a.message_uuid = $1
                AND a.generation = o.generation
         ) AS fresh
     ), written AS (
         INSERT INTO mailbox.outbox_stages (message_uuid, stage, attempt_uuid, evidence)
         SELECT $1, $2, $3, $4 FROM fence WHERE fresh
         ON CONFLICT (message_uuid, stage) DO NOTHING
         RETURNING 1
     )
     SELECT fence.fresh, EXISTS (SELECT 1 FROM written) FROM fence";

pub(super) const SETTLE: &str = "UPDATE mailbox.outbox
    SET status = $2, status_reason = $3, updated_at = now()
  WHERE message_uuid = $1 AND status <> 'delivered'";

pub(super) const STAGES: &str = "SELECT stage, attempt_uuid, evidence FROM mailbox.outbox_stages
  WHERE message_uuid = $1
  ORDER BY array_position(
      ARRAY['queued', 'accepted', 'consumed', 'action_started', 'completed'], stage)";
