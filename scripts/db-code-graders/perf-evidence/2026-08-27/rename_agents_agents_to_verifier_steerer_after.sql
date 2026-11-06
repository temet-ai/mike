UPDATE agents.agents
   SET name = 'verifier-steerer',
       description = 'Verifier steerer — detects unbacked completion claims and injects a [verification] nudge',
       updated_at = NOW()
 WHERE name = 'false-done';
