select coalesce(json_agg(x order by x.created_at), '[]') from (
  select n.kind, n.status, n.title, n.data, n.created_at, l.seat
    from workspace.session_notes n
    join sessions.sessions s on s.id = n.session_id
    join launch.launches l on l.session_uuid = s.session_uuid and l.run_id = 'fleet-judgement-at-work-541ce7e3'
   where n.kind in ('decision', 'task') and n.created_at <= now()) x
