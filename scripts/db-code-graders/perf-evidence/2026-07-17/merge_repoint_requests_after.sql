UPDATE requests.requests SET project_id = 13 WHERE project_id = ANY(ARRAY[2]::bigint[])
