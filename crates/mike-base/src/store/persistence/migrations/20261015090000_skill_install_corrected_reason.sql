ALTER TABLE memory.skill_installs
    DROP CONSTRAINT skill_installs_retired_reason_check,
    ADD CONSTRAINT skill_installs_retired_reason_check
        CHECK (retired_reason IN ('unloaded', 'corrected_above_base', 'user', 'policy'));

COMMENT ON TABLE memory.skill_installs IS
    'What the learning promoter last wrote into project checkouts on each machine, reported '
    'by the promoter at the end of every run: the SKILL.md path, the draft it came from (NULL '
    'for the generated skill), when it was first written and, once removed, when and why '
    '(unloaded: no load in 30 days; corrected_above_base: at least 10 loads and the turns that '
    'loaded it were corrected more often than the project''s turns over the same window; user: '
    'retired on the Skills page; policy: the project asks first and the user has not approved it).';
