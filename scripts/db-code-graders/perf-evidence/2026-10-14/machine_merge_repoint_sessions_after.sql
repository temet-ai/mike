UPDATE sessions.sessions s SET location_uuid = p.winner
  FROM unnest(ARRAY['2c90ac69-b1f6-44a9-a16c-7d8e09c3a3ee']::uuid[], ARRAY['9b598246-10ef-4549-a3be-73ba5dcdd650']::uuid[]) AS p(loser, winner)
 WHERE s.location_uuid = p.loser
