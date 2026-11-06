UPDATE steering.interventions i
   SET acked_at = COALESCE(i.acked_at, now())
  FROM remote_control.injections inj
  JOIN remote_control.channels c ON c.id = inj.channel_id
 WHERE inj.event_id = ANY(ARRAY['44444444-4444-4444-4444-444444444444']::uuid[])
   AND c.cse_id = 'cse_01VK2nJHVFMa6ahVpG8Cnnuw'
   AND inj.source_kind = 'steerer'
   AND inj.source_id IS NOT NULL
   AND i.uuid = inj.source_id::uuid
