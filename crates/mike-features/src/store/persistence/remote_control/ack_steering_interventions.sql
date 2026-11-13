UPDATE steering.interventions i
   SET acked_at = COALESCE(i.acked_at, $3)
  FROM remote_control.injections inj
  JOIN remote_control.channels c ON c.id = inj.channel_id
 WHERE inj.event_id = ANY($2::uuid[])
   AND c.cse_id = $1
   AND inj.source_kind = 'steerer'
   AND inj.source_id IS NOT NULL
   AND i.uuid = inj.source_id::uuid
