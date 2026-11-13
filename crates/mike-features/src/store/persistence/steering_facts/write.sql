UPDATE memory.turn_facts f
    SET wait_backing = x.wait_backing,
        wait_backing_refs = x.wait_backing_refs,
        observed_after_change = x.observed_after_change,
        last_change_use_id = x.last_change_use_id,
        last_change_at = x.last_change_at,
        last_observation_use_id = x.last_observation_use_id,
        last_observation_at = x.last_observation_at,
        observation_kind = x.observation_kind,
        steering_facts_version = x.steering_facts_version
   FROM jsonb_to_recordset($1::jsonb) AS x(
        operator_turn_id bigint, wait_backing text, wait_backing_refs jsonb,
        observed_after_change boolean, last_change_use_id bigint, last_change_at timestamptz,
        last_observation_use_id bigint, last_observation_at timestamptz, observation_kind text,
        steering_facts_version smallint)
  WHERE f.operator_turn_id = x.operator_turn_id
    AND (f.wait_backing, f.wait_backing_refs, f.observed_after_change, f.last_change_use_id,
         f.last_observation_use_id, f.observation_kind, f.steering_facts_version)
        IS DISTINCT FROM
        (x.wait_backing, x.wait_backing_refs, x.observed_after_change, x.last_change_use_id,
         x.last_observation_use_id, x.observation_kind, x.steering_facts_version)
