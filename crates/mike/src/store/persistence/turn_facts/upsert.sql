WITH cleared AS (
        DELETE FROM memory.turn_facts_backstop_attempts a
         USING jsonb_to_recordset($1::jsonb) AS w(operator_turn_id bigint)
         WHERE a.operator_turn_id = w.operator_turn_id
 ), written AS (
 INSERT INTO memory.turn_facts (
        operator_turn_id, session_id, builder_version, built_at, max_request_id,
        prompt_kind, prompt_rule, prompt_kind_at_close, prompt_rule_at_close,
        next_state, next_genuine_request_id, next_genuine_turn_id, next_genuine_at,
        next_state_at_close, next_genuine_request_id_at_close,
        skipped_before_next, reply_window_segment_ids, reclassified_at,
        text_chars, text_cut_at, last_text_origin, text_spans,
        stop_request_id, stop_reason, aborted, interrupted, close_cause,
        checks_run, checks_count, checks_failed,
        ended_on, ended_on_version, session_class, session_class_rule)
 SELECT x.operator_turn_id, x.session_id, x.builder_version, x.built_at, x.max_request_id,
        x.prompt_kind, x.prompt_rule, x.prompt_kind_at_close, x.prompt_rule_at_close,
        x.next_state, x.next_genuine_request_id, x.next_genuine_turn_id, x.next_genuine_at,
        x.next_state_at_close, x.next_genuine_request_id_at_close,
        x.skipped_before_next, x.reply_window_segment_ids, x.reclassified_at,
        x.text_chars, x.text_cut_at, x.last_text_origin, x.text_spans,
        x.stop_request_id, x.stop_reason, x.aborted, x.interrupted, x.close_cause,
        x.checks_run, x.checks_count, x.checks_failed,
        x.ended_on, x.ended_on_version, x.session_class, x.session_class_rule
   FROM jsonb_to_recordset($1::jsonb) AS x(
        operator_turn_id bigint, session_id bigint, builder_version text,
        built_at timestamptz, max_request_id bigint,
        prompt_kind text, prompt_rule text, prompt_kind_at_close text,
        prompt_rule_at_close text,
        next_state text, next_genuine_request_id bigint, next_genuine_turn_id bigint,
        next_genuine_at timestamptz, next_state_at_close text,
        next_genuine_request_id_at_close bigint,
        skipped_before_next jsonb, reply_window_segment_ids bigint[],
        reclassified_at timestamptz,
        text_chars integer, text_cut_at integer, last_text_origin text, text_spans jsonb,
        stop_request_id bigint, stop_reason text, aborted boolean, interrupted boolean,
        close_cause text,
        checks_run jsonb, checks_count integer, checks_failed integer,
        ended_on text, ended_on_version text, session_class text, session_class_rule text)
 ON CONFLICT (operator_turn_id) DO UPDATE SET
        builder_version = EXCLUDED.builder_version,
        built_at = EXCLUDED.built_at,
        max_request_id = EXCLUDED.max_request_id,
        prompt_kind = EXCLUDED.prompt_kind,
        prompt_rule = EXCLUDED.prompt_rule,
        next_state = EXCLUDED.next_state,
        next_genuine_request_id = EXCLUDED.next_genuine_request_id,
        next_genuine_turn_id = EXCLUDED.next_genuine_turn_id,
        next_genuine_at = EXCLUDED.next_genuine_at,
        skipped_before_next = EXCLUDED.skipped_before_next,
        reply_window_segment_ids = EXCLUDED.reply_window_segment_ids,
        reclassified_at = CASE
            WHEN memory.turn_facts.prompt_kind IS DISTINCT FROM EXCLUDED.prompt_kind
              OR memory.turn_facts.prompt_rule IS DISTINCT FROM EXCLUDED.prompt_rule
              OR (memory.turn_facts.next_genuine_request_id IS NOT NULL
                  AND memory.turn_facts.next_genuine_request_id
                      IS DISTINCT FROM EXCLUDED.next_genuine_request_id)
            THEN now()
            ELSE memory.turn_facts.reclassified_at
        END,
        text_chars = EXCLUDED.text_chars,
        text_cut_at = EXCLUDED.text_cut_at,
        last_text_origin = EXCLUDED.last_text_origin,
        text_spans = EXCLUDED.text_spans,
        stop_request_id = EXCLUDED.stop_request_id,
        stop_reason = EXCLUDED.stop_reason,
        aborted = EXCLUDED.aborted,
        interrupted = EXCLUDED.interrupted,
        close_cause = EXCLUDED.close_cause,
        checks_run = EXCLUDED.checks_run,
        checks_count = EXCLUDED.checks_count,
        checks_failed = EXCLUDED.checks_failed,
        ended_on = EXCLUDED.ended_on,
        ended_on_version = EXCLUDED.ended_on_version,
        session_class = EXCLUDED.session_class,
        session_class_rule = EXCLUDED.session_class_rule
  WHERE memory.turn_facts.built_at <= EXCLUDED.built_at
    AND (memory.turn_facts.builder_version, memory.turn_facts.max_request_id,
         memory.turn_facts.prompt_kind, memory.turn_facts.prompt_rule,
         memory.turn_facts.next_state, memory.turn_facts.next_genuine_request_id,
         memory.turn_facts.next_genuine_turn_id, memory.turn_facts.next_genuine_at,
         memory.turn_facts.skipped_before_next, memory.turn_facts.reply_window_segment_ids,
         memory.turn_facts.text_chars, memory.turn_facts.text_cut_at,
         memory.turn_facts.last_text_origin, memory.turn_facts.text_spans,
         memory.turn_facts.stop_request_id, memory.turn_facts.stop_reason,
         memory.turn_facts.aborted, memory.turn_facts.interrupted,
         memory.turn_facts.close_cause, memory.turn_facts.checks_run,
         memory.turn_facts.checks_count, memory.turn_facts.checks_failed,
         memory.turn_facts.ended_on, memory.turn_facts.ended_on_version,
         memory.turn_facts.session_class, memory.turn_facts.session_class_rule)
        IS DISTINCT FROM
        (EXCLUDED.builder_version, EXCLUDED.max_request_id,
         EXCLUDED.prompt_kind, EXCLUDED.prompt_rule,
         EXCLUDED.next_state, EXCLUDED.next_genuine_request_id,
         EXCLUDED.next_genuine_turn_id, EXCLUDED.next_genuine_at,
         EXCLUDED.skipped_before_next, EXCLUDED.reply_window_segment_ids,
         EXCLUDED.text_chars, EXCLUDED.text_cut_at,
         EXCLUDED.last_text_origin, EXCLUDED.text_spans,
         EXCLUDED.stop_request_id, EXCLUDED.stop_reason,
         EXCLUDED.aborted, EXCLUDED.interrupted,
         EXCLUDED.close_cause, EXCLUDED.checks_run,
         EXCLUDED.checks_count, EXCLUDED.checks_failed,
         EXCLUDED.ended_on, EXCLUDED.ended_on_version,
         EXCLUDED.session_class, EXCLUDED.session_class_rule)
 RETURNING memory.turn_facts.operator_turn_id AS operator_turn_id,
           (old.operator_turn_id IS NULL) AS inserted,
           (memory.turn_facts.reclassified_at IS NOT DISTINCT FROM now()) AS reclassified
 ), queued AS (
 UPDATE memory.operator_turns t
    SET judge_queued_at = CASE WHEN e.eligible THEN COALESCE(t.judge_queued_at, now()) END
   FROM (SELECT w.operator_turn_id,
                (ot.closed_at IS NOT NULL
                 AND x.prompt_kind = 'genuine'
                 AND x.session_class <> 'synthetic'
                 AND r.claude_code_agent_id IS NULL
                 AND x.next_state IN ('resolved', 'none_session_settled')
                 AND ot.judge_failure_reason IS DISTINCT FROM 'permanent'
                 AND NOT EXISTS (
                     SELECT 1 FROM memory.turn_verdicts v
                      WHERE v.operator_turn_id = ot.id
                        AND v.mode = 'live'
                        AND v.reaction IS NOT NULL
                        AND v.next_genuine_request_id IS NOT DISTINCT FROM x.next_genuine_request_id)
                ) AS eligible
           FROM written w
           JOIN jsonb_to_recordset($1::jsonb) AS x(
                operator_turn_id bigint, prompt_kind text, session_class text,
                next_state text, next_genuine_request_id bigint)
             ON x.operator_turn_id = w.operator_turn_id
           JOIN memory.operator_turns ot ON ot.id = w.operator_turn_id
           JOIN requests.requests r ON r.id = ot.start_request_id) e
  WHERE t.id = e.operator_turn_id
    AND (t.judge_queued_at IS NOT NULL) <> e.eligible
 )
 SELECT inserted AS "inserted!", reclassified AS "reclassified!" FROM written
