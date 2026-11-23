SELECT r.session_id                AS "session_id!",
       r.lease_expires_at          AS "lease_expires_at!",
       sg.goal                     AS "goal?",
       sg.next_steps               AS "next_steps?",
       sg.display_name             AS "display_name?",
       sg.role                     AS "role?",
       sg.instructions             AS "instructions?",
       v.newest_request_number     AS "newest_request_number?",
       s.machine_uuid              AS "machine_uuid?",
       sm.name                     AS "machine_name?",
       v.session_uuid              AS "session_uuid!",
       v.runtime                   AS "runtime!",
       v.source                    AS "source!",
       v.source_version            AS "source_version?",
       v.fidelity                  AS "fidelity!",
       v.completeness              AS "completeness!",
       v.observed_at               AS "observed_at?",
       v.lifecycle                 AS "lifecycle?",
       v.lifecycle_at              AS "lifecycle_at?",
       v.control                   AS "control?",
       v.requested_model           AS "requested_model?",
       v.observed_model            AS "observed_model?",
       v.usage_scope               AS "usage_scope?",
       v.usage_basis               AS "usage_basis?",
       v.input_tokens              AS "input_tokens?",
       v.cached_input_tokens       AS "cached_input_tokens?",
       v.output_tokens             AS "output_tokens?",
       v.reasoning_output_tokens   AS "reasoning_output_tokens?",
       v.total_tokens              AS "total_tokens?",
       v.context_window            AS "context_window?",
       v.model_call_count          AS "model_call_count!",
       ch.id                       AS "channel_id?",
       ch.last_heartbeat_at        AS "last_heartbeat_at?",
       ch.last_heartbeat_interval_seconds AS "last_heartbeat_interval_seconds?",
       ch.last_idle_seconds        AS "last_idle_seconds?",
       ch.worker_status            AS "worker_status?",
       ch.worker_status_at         AS "worker_status_at?",
       ch.stream_open              AS "stream_open?",
       ch.stream_opened_at         AS "stream_opened_at?",
       ch.last_delivery_at         AS "last_delivery_at?",
       ch.post_turn_summary        AS "post_turn_summary?",
       ch.post_turn_summary_at     AS "post_turn_summary_at?"
  FROM workspace.session_registrations r
  JOIN sessions.sessions s ON s.id = r.session_id
  JOIN runtime.session_observation_summary_v v ON v.session_id = r.session_id
  LEFT JOIN projects.machines sm ON sm.machine_uuid = s.machine_uuid
  LEFT JOIN sessions.session_goals sg ON sg.session_id = r.session_id
  LEFT JOIN LATERAL (
       SELECT c.id, c.last_heartbeat_at, c.last_heartbeat_interval_seconds,
              c.last_idle_seconds, c.worker_status, c.worker_status_at,
              (c.stream_opened_at IS NOT NULL
                 AND (c.stream_closed_at IS NULL
                      OR c.stream_closed_at < c.stream_opened_at)) AS stream_open,
              c.stream_opened_at, c.last_delivery_at,
              c.post_turn_summary, c.post_turn_summary_at
         FROM remote_control.channels c
        WHERE c.id = ANY($2::bigint[])
          AND c.local_session_uuid = v.session_uuid
        ORDER BY c.last_heartbeat_at DESC NULLS LAST, c.id
        LIMIT 1
  ) ch ON true
 WHERE r.session_id = ANY($1::bigint[])
