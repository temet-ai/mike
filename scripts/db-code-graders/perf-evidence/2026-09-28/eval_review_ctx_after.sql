SELECT t.id AS operator_turn_id, s.session_uuid, t.closed_at,
        t.user_prompt, t.final_assistant_text, (f.operator_turn_id IS NOT NULL) AS facts_present,
        f.text_chars, f.text_cut_at, t.tool_call_count, t.tool_error_count, f.next_state,
        f.next_genuine_turn_id, f.next_genuine_request_id, f.next_genuine_at,
        COALESCE(nt.user_prompt, nr.human_prompt) AS next_genuine_text
   FROM memory.operator_turns t
   JOIN sessions.sessions s ON s.id = t.session_id
   LEFT JOIN memory.turn_facts f ON f.operator_turn_id = t.id
   LEFT JOIN memory.operator_turns nt ON nt.id = f.next_genuine_turn_id
   LEFT JOIN requests.requests nr ON nr.id = f.next_genuine_request_id
  WHERE t.id = 13778
