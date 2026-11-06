SELECT name, id FROM agents.agents WHERE name = ANY(ARRAY['db-dev','rust-developer','technical-writer']::TEXT[])
