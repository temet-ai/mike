SELECT CASE WHEN l.last_seen > c.last_seen THEN c.location_uuid ELSE l.location_uuid END,
       CASE WHEN l.last_seen > c.last_seen THEN l.location_uuid ELSE c.location_uuid END
  FROM projects.locations l
  JOIN projects.locations c ON c.root_path = l.root_path AND c.machine_uuid = 'b2923f57-bba3-455f-a572-6a3b22027c4d'
 WHERE l.machine_uuid = 'fd7c84a7-113a-45d4-9433-86def71bd0d8'
   FOR UPDATE OF l, c
