SELECT to_jsonb(r) AS "record!: serde_json::Value"
  FROM launch.run_records r
 WHERE r.fleet_uuid = $1
