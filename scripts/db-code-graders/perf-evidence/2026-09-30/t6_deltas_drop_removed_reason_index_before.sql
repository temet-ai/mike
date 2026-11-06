INSERT INTO pf.deltas_before (id, request_id, predecessor_request_id, session_id, message_id, change_kind_id, ord, removed_reason_id)
SELECT 4000000 + g, 4000000 + g, 3999999 + g, 13000 + g / 8, 5000000 + g * 37, CASE WHEN g % 10 < 7 THEN 2 ELSE 1 END, g % 40, CASE WHEN g % 10 < 7 THEN 1 + g % 3 ELSE NULL END
FROM generate_series(1, 16) g
