SELECT r.session_id,
        ro.code AS turn_origin,
        tt.code AS turn_type,
        r.claude_code_agent_id IS NOT NULL AS has_agent_id,
        r.human_prompt,
        r.last_user_message,
        ak.code AS author
   FROM requests.requests r
   JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
   JOIN requests.turn_types tt ON tt.id = r.turn_type_id
   JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
  WHERE r.id = 447618
