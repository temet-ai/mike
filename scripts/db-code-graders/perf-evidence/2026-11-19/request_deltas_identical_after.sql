SELECT dck.code AS kind, d.ord, drr.code AS reason,
       d.sent_patch_start, d.sent_patch_end, d.sent_patch_bytes,
       d.predecessor_request_id, d.session_id
  FROM requests.deltas d
  JOIN requests.delta_change_kinds dck ON dck.id = d.change_kind_id
  LEFT JOIN requests.delta_removed_reasons drr ON drr.id = d.removed_reason_id
 WHERE d.request_id = 855145
