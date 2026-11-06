SELECT state.namespace_id AS "namespace_id!",
       state.thread_id AS "thread_id!",
       state.turn_id AS "turn_id!",
       state.item_id AS "item_id!",
       state.first_capture_sequence AS "first_capture_sequence!",
       state.latest_capture_sequence AS "latest_capture_sequence!",
       state.state_codec_version AS "state_codec_version!",
       state.state_document::text AS "state_document!"
  FROM runtime.codex_item_state state
 WHERE state.generation_id = $1 AND state.thread_id = $2
   AND (state.first_capture_sequence, state.turn_id, state.item_id) > ($3::bigint, $4::text, $5::text)
 ORDER BY state.first_capture_sequence, state.turn_id, state.item_id
 LIMIT $6
