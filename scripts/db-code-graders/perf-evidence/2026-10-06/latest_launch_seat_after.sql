SELECT run_id, seat, (closed_at IS NOT NULL) AS closed FROM launch.launches WHERE session_uuid = 'b98598cb-8429-407f-a224-fa8beaaa0cf8' ORDER BY created_at DESC, id DESC LIMIT 1
