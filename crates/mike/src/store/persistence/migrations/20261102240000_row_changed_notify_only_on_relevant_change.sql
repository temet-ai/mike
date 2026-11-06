-- `proxy.row_changed` notifies only when a consumer can see the change.
--
-- Every transaction that calls pg_notify takes a database-wide lock at commit
-- (AccessExclusiveLock on object 0 of class 1262), so a notification nobody acts
-- on still serialises commits across the whole database. Before this migration
-- every row trigger fired on every UPDATE, including no-op updates, `last_seen`
-- bumps, body compaction (request_body -> NULL), session re-projection (every
-- request of a session) and embedding refreshes of unchanged input.
--
-- Consumers: the SPA through `/api/ws` (frontend/src/lib/stores/*,
-- lib/comms/*, lib/session-flow/liveAdapter.ts, shell panels, fleet page) and
-- `change_feed` (work_registry_worker watches `requests` per session).
--
-- INSERT and DELETE keep notifying unconditionally. UPDATE notifies only when a
-- column some consumer reads changes:
--   requests            status, completion, turn/agent/origin links, intercept
--                       and the row's identity columns. Not request_body /
--                       request_envelope (compaction, envelope backfill),
--                       project_id (session re-projection; the sessions row
--                       announces it), predecessor_request_id / request_number /
--                       sent_patch_* (written in the insert transaction, which
--                       already notifies INSERT, or by backfills).
--   sessions            every column but last_seen. A last_seen bump comes with
--                       a requests INSERT or a last_status change, both of
--                       which already notify.
--   session_embeddings  the embedded input (source, source_updated_at,
--                       chunk_count, embedding_model), not embedded_at, so
--                       re-embedding unchanged input is silent.
--   mailbox.messages    delivery state the SPA shows, not claimed_at /
--                       next_retry_at (claim and retry bookkeeping).
--   all other tables    any column (`OLD.* IS DISTINCT FROM NEW.*`): no-op
--                       updates are silent.
--
-- Payloads: sessions now carries `session_id` (its own uuid) and interventions
-- carries its session's uuid, so session-scoped consumers (change_feed, the
-- Flow live adapter, the steering panels) can scope them. The session lookup
-- runs only when the trigger fires.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_requests()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
    FROM sessions.sessions s
    WHERE s.id = COALESCE(NEW.session_id, OLD.session_id);
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'requests',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_requests_row_changed
AFTER INSERT OR DELETE ON requests.requests
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_requests();

CREATE OR REPLACE TRIGGER trg_requests_row_changed_update
AFTER UPDATE ON requests.requests
FOR EACH ROW
WHEN ((OLD.status_id, OLD.completed_at, OLD.operator_turn_id, OLD.agent_instance_id,
       OLD.turn_origin_id, OLD.intercepted, OLD.correction_request_id, OLD.session_id,
       OLD.model, OLD.turn_type_id, OLD.subagent_persona, OLD.claude_code_agent_id,
       OLD.is_streaming, OLD.message_count, OLD.persisted_at)
      IS DISTINCT FROM
      (NEW.status_id, NEW.completed_at, NEW.operator_turn_id, NEW.agent_instance_id,
       NEW.turn_origin_id, NEW.intercepted, NEW.correction_request_id, NEW.session_id,
       NEW.model, NEW.turn_type_id, NEW.subagent_persona, NEW.claude_code_agent_id,
       NEW.is_streaming, NEW.message_count, NEW.persisted_at))
EXECUTE FUNCTION mike.notify_row_changed_requests();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_sessions()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'sessions',
        'id',         COALESCE(NEW.session_uuid, OLD.session_uuid)::text,
        'session_id', COALESCE(NEW.session_uuid, OLD.session_uuid)::text,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_sessions_row_changed
AFTER UPDATE ON sessions.sessions
FOR EACH ROW
WHEN ((OLD.last_status, OLD.title, OLD.project_id, OLD.project_resolution,
       OLD.location_uuid, OLD.cwd, OLD.pinned, OLD.pin_created_at, OLD.ttl_override,
       OLD.ttl_override_set_at, OLD.overview_generated_at, OLD.flow_capture_version,
       OLD.flow_admitted_at, OLD.flow_producer, OLD.flow_hook_event_name,
       OLD.flow_start_source, OLD.first_seen, OLD.session_uuid, OLD.overview)
      IS DISTINCT FROM
      (NEW.last_status, NEW.title, NEW.project_id, NEW.project_resolution,
       NEW.location_uuid, NEW.cwd, NEW.pinned, NEW.pin_created_at, NEW.ttl_override,
       NEW.ttl_override_set_at, NEW.overview_generated_at, NEW.flow_capture_version,
       NEW.flow_admitted_at, NEW.flow_producer, NEW.flow_hook_event_name,
       NEW.flow_start_source, NEW.first_seen, NEW.session_uuid, NEW.overview))
EXECUTE FUNCTION mike.notify_row_changed_sessions();

CREATE OR REPLACE TRIGGER trg_session_embeddings_row_changed
AFTER INSERT OR DELETE ON memory.session_embeddings
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_session_embeddings();

CREATE OR REPLACE TRIGGER trg_session_embeddings_row_changed_update
AFTER UPDATE ON memory.session_embeddings
FOR EACH ROW
WHEN ((OLD.source_updated_at, OLD.source, OLD.chunk_count, OLD.embedding_model)
      IS DISTINCT FROM
      (NEW.source_updated_at, NEW.source, NEW.chunk_count, NEW.embedding_model))
EXECUTE FUNCTION mike.notify_row_changed_session_embeddings();

CREATE OR REPLACE TRIGGER trg_mailbox_messages_row_changed
AFTER INSERT OR DELETE ON mailbox.messages
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_mailbox_messages();

CREATE OR REPLACE TRIGGER trg_mailbox_messages_row_changed_update
AFTER UPDATE ON mailbox.messages
FOR EACH ROW
WHEN ((OLD.sent_at, OLD.consumed_at, OLD.dead_lettered_at, OLD.delivery_attempts,
       OLD.channel_delivered_at, OLD.sent_request_id, OLD.thread_id, OLD.in_reply_to_id,
       OLD.to_session_id, OLD.from_session_id, OLD.queued_at, OLD.body)
      IS DISTINCT FROM
      (NEW.sent_at, NEW.consumed_at, NEW.dead_lettered_at, NEW.delivery_attempts,
       NEW.channel_delivered_at, NEW.sent_request_id, NEW.thread_id, NEW.in_reply_to_id,
       NEW.to_session_id, NEW.from_session_id, NEW.queued_at, NEW.body))
EXECUTE FUNCTION mike.notify_row_changed_mailbox_messages();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_interventions()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
    FROM sessions.sessions s
    WHERE s.id = COALESCE(NEW.session_id, OLD.session_id);
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'interventions',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_interventions_row_changed
AFTER INSERT OR DELETE ON steering.interventions
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_interventions();

CREATE OR REPLACE TRIGGER trg_interventions_row_changed_update
AFTER UPDATE ON steering.interventions
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_interventions();

CREATE OR REPLACE TRIGGER trg_mailbox_threads_row_changed
AFTER INSERT OR DELETE ON mailbox.threads
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_mailbox_threads();

CREATE OR REPLACE TRIGGER trg_mailbox_threads_row_changed_update
AFTER UPDATE ON mailbox.threads
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_mailbox_threads();

CREATE OR REPLACE TRIGGER trg_consolidation_actions_row_changed
AFTER INSERT OR DELETE ON memory.consolidation_actions
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_consolidation_actions();

CREATE OR REPLACE TRIGGER trg_consolidation_actions_row_changed_update
AFTER UPDATE ON memory.consolidation_actions
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_consolidation_actions();

CREATE OR REPLACE TRIGGER trg_consolidation_runs_row_changed
AFTER INSERT OR DELETE ON memory.consolidation_runs
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_consolidation_runs();

CREATE OR REPLACE TRIGGER trg_consolidation_runs_row_changed_update
AFTER UPDATE ON memory.consolidation_runs
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_consolidation_runs();

CREATE OR REPLACE TRIGGER trg_lesson_injections_row_changed
AFTER INSERT OR DELETE ON memory.lesson_injections
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_lesson_injections();

CREATE OR REPLACE TRIGGER trg_lesson_injections_row_changed_update
AFTER UPDATE ON memory.lesson_injections
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_lesson_injections();

CREATE OR REPLACE TRIGGER trg_session_judges_verdicts_row_changed
AFTER INSERT OR DELETE ON session_judges.verdicts
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_session_judges_verdicts();

CREATE OR REPLACE TRIGGER trg_session_judges_verdicts_row_changed_update
AFTER UPDATE ON session_judges.verdicts
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_session_judges_verdicts();

CREATE OR REPLACE TRIGGER trg_llm_calls_row_changed
AFTER INSERT OR DELETE ON llm.calls
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_llm_calls();

CREATE OR REPLACE TRIGGER trg_llm_calls_row_changed_update
AFTER UPDATE ON llm.calls
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_llm_calls();

CREATE OR REPLACE TRIGGER trg_filter_rules_row_changed
AFTER INSERT OR DELETE ON rules.rules
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_filter_rules();

CREATE OR REPLACE TRIGGER trg_filter_rules_row_changed_update
AFTER UPDATE ON rules.rules
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_filter_rules();

CREATE OR REPLACE TRIGGER trg_prompt_compositions_row_changed
AFTER INSERT OR DELETE ON overrides.overrides
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_prompt_compositions();

CREATE OR REPLACE TRIGGER trg_prompt_compositions_row_changed_update
AFTER UPDATE ON overrides.overrides
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_prompt_compositions();

CREATE OR REPLACE TRIGGER trg_steering_evaluations_row_changed
AFTER INSERT OR DELETE ON steering.evaluations
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_steering_evaluations();

CREATE OR REPLACE TRIGGER trg_steering_evaluations_row_changed_update
AFTER UPDATE ON steering.evaluations
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_steering_evaluations();

CREATE OR REPLACE TRIGGER trg_launch_launches_row_changed
AFTER INSERT OR DELETE ON launch.launches
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_launch_launches();

CREATE OR REPLACE TRIGGER trg_launch_launches_row_changed_update
AFTER UPDATE ON launch.launches
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_launch_launches();
