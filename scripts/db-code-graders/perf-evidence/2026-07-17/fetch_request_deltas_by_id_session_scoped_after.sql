SELECT rd.request_id, rd.predecessor_request_id,
       ck.code AS change_kind, rd.ord,
       rd.message_id, m.message_sha256,
       ro.code AS role, rd.byte_size, rd.body,
       rr.code AS removed_reason, rd.computed_at
  FROM requests.deltas rd
  JOIN messages.messages m ON m.id = rd.message_id
  JOIN requests.delta_change_kinds ck ON ck.id = rd.change_kind_id
  JOIN messages.roles ro ON ro.id = rd.role_id
  LEFT JOIN requests.delta_removed_reasons rr ON rr.id = rd.removed_reason_id
 WHERE rd.request_id = 1
   AND rd.session_id = 1
 ORDER BY ck.code ASC, rd.ord ASC;
