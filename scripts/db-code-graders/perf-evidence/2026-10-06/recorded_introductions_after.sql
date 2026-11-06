SELECT again_use_id FROM launch.introductions WHERE again_use_id = ANY(ARRAY[1005,1100,1412,99999]::bigint[])
