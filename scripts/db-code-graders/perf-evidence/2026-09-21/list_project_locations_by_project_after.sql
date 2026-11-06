SELECT l.location_uuid, l.machine_uuid, m.name AS machine_name, l.root_path, l.last_seen
         FROM projects.locations l
         JOIN projects.machines m ON m.machine_uuid = l.machine_uuid
         WHERE l.project_id = 313870
         ORDER BY m.name, l.root_path, l.location_uuid;
