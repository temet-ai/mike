SELECT count(*) FROM memory.turn_facts
WHERE NOT (observation_kind IS NULL
           OR observation_kind IN ('test', 'build', 'lint', 'gate', 'run', 'http', 'browser',
                                   'render', 'deploy_poll', 'read_back'));
