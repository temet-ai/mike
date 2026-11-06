INSERT INTO requests.author_kinds (id, code) OVERRIDING SYSTEM VALUE
VALUES (102, 'test-default-perf-check')
ON CONFLICT (id) DO NOTHING;
