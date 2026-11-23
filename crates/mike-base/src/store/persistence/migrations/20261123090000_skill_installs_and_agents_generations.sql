-- Two change counters in memory.change_generations, so the lesson worker keeps
-- the topic-withheld lesson ids and the agent alias map in memory and reads
-- them again only when their sources change, instead of on every refresh
-- (600 reads an hour each across the machines).
--
-- skill_installs: a skill folder installed, retired or re-watermarked.
-- agents: an agent added or removed, or its name, persona aliases or name
-- variants changed.
--
-- The withheld set also reads memory.lessons, which the existing 'lessons'
-- counter already covers.
INSERT INTO memory.change_generations (name) VALUES ('skill_installs'), ('agents')
ON CONFLICT (name) DO NOTHING;

DROP TRIGGER IF EXISTS trg_skill_installs_change_generation ON memory.skill_installs;
DROP TRIGGER IF EXISTS trg_skill_installs_change_generation_upd ON memory.skill_installs;
CREATE TRIGGER trg_skill_installs_change_generation
    AFTER INSERT OR DELETE ON memory.skill_installs
    FOR EACH ROW EXECUTE FUNCTION memory.bump_change_generation('skill_installs');
CREATE TRIGGER trg_skill_installs_change_generation_upd
    AFTER UPDATE OF project_id, folder, lesson_watermark, retired_at ON memory.skill_installs
    FOR EACH ROW
    WHEN ((OLD.project_id, OLD.folder, OLD.lesson_watermark, OLD.retired_at)
          IS DISTINCT FROM (NEW.project_id, NEW.folder, NEW.lesson_watermark, NEW.retired_at))
    EXECUTE FUNCTION memory.bump_change_generation('skill_installs');

DROP TRIGGER IF EXISTS trg_agents_change_generation ON agents.agents;
DROP TRIGGER IF EXISTS trg_agents_change_generation_upd ON agents.agents;
CREATE TRIGGER trg_agents_change_generation
    AFTER INSERT OR DELETE ON agents.agents
    FOR EACH ROW EXECUTE FUNCTION memory.bump_change_generation('agents');
CREATE TRIGGER trg_agents_change_generation_upd
    AFTER UPDATE OF name, persona_aliases, name_variants ON agents.agents
    FOR EACH ROW
    WHEN ((OLD.name, OLD.persona_aliases, OLD.name_variants)
          IS DISTINCT FROM (NEW.name, NEW.persona_aliases, NEW.name_variants))
    EXECUTE FUNCTION memory.bump_change_generation('agents');
