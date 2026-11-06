SELECT DISTINCT session_uuid FROM launch.launches WHERE session_uuid = ANY(ARRAY['b98598cb-8429-407f-a224-fa8beaaa0cf8'::uuid,'7ca78497-2ec3-4596-87d2-28aeacb454fe'::uuid]) AND closed_at IS NULL
