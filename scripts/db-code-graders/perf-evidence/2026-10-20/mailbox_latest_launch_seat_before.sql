SELECT run_id, seat, (closed_at IS NOT NULL) AS closed FROM launch.launches WHERE session_uuid = '2a62d305-cd85-452a-80e1-50273f7768cb'::uuid ORDER BY created_at DESC, id DESC LIMIT 1
