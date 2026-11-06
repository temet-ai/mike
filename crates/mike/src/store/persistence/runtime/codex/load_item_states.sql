SELECT state.namespace_id AS "namespace_id!",
       state.thread_id AS "thread_id!",
       state.turn_id AS "turn_id!",
       state.item_id AS "item_id!",
       state.first_capture_sequence AS "first_capture_sequence!",
       state.latest_capture_sequence AS "latest_capture_sequence!",
       state.state_codec_version AS "state_codec_version!",
       state.state_document::text AS "state_document!"
  FROM runtime.codex_item_state state
  JOIN (SELECT DISTINCT thread_id, turn_id, item_id
          FROM UNNEST($2::text[], $3::text[], $4::text[]) AS keys(thread_id, turn_id, item_id)) wanted
 USING (thread_id, turn_id, item_id)
 WHERE state.generation_id = $1
