SELECT operator_turn_id, session_id, builder_version, built_at, max_request_id,
       prompt_kind, prompt_rule, prompt_kind_at_close, prompt_rule_at_close,
       next_state, next_genuine_request_id, next_genuine_turn_id, next_genuine_at,
       next_state_at_close, next_genuine_request_id_at_close,
       skipped_before_next, reply_window_segment_ids, reclassified_at,
       text_chars, text_cut_at, last_text_origin, text_spans,
       stop_request_id, stop_reason, aborted, interrupted, close_cause,
       checks_run, checks_count, checks_failed,
       ended_on, ended_on_version, session_class, session_class_rule
  FROM memory.turn_facts
 WHERE session_id = ANY($1)
 ORDER BY session_id, operator_turn_id
