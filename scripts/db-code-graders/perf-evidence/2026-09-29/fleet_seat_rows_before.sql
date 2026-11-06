with seats as (
  select l.seat, l.agent, l.model, l.session_uuid, l.created_at, l.closed_at, l.surface_ref,
         s.id sid, g.role, g.goal
    from launch.launches l
    left join sessions.sessions s on s.session_uuid = l.session_uuid
    left join sessions.session_goals g on g.session_id = s.id
   where l.run_id = 'fleet-judgement-at-work-541ce7e3' and l.opened and l.created_at <= now()
)
select coalesce(json_agg(x order by x.created_at), '[]') from (
  select seat, agent, model, session_uuid, created_at, surface_ref, role, goal,
         case when closed_at <= now() then closed_at end closed_at,
         (select count(*) from requests.requests r
           where r.session_id = seats.sid
             and r.created_at between seats.created_at - interval '1 hour' and now()) requests,
         (select max(r.created_at) from requests.requests r
           where r.session_id = seats.sid
             and r.created_at between seats.created_at - interval '1 hour' and now()) last_request_at,
         (select rs.input_tokens + coalesce(rs.cache_read_input_tokens, 0)
                                 + coalesce(rs.cache_creation_input_tokens, 0)
            from requests.requests r join responses.responses rs on rs.request_id = r.id
           where r.session_id = seats.sid and r.subagent_persona is null
             and rs.input_tokens is not null
             and r.created_at between seats.created_at - interval '1 hour' and now()
           order by r.id desc limit 1) context_tokens
    from seats) x
