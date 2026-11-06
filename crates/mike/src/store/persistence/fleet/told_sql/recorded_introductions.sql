SELECT again_use_id
  FROM launch.introductions
 WHERE again_use_id = ANY($1::bigint[])
