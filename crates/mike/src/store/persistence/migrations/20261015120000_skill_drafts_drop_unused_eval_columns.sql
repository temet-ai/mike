ALTER TABLE memory.skill_drafts
    DROP COLUMN test_prompts,
    DROP COLUMN assertions;

COMMENT ON TABLE memory.skill_drafts IS
    'A skill drafted by the skill author pass from one topic''s accepted lessons, '
    'following the skill-creator method: a short SKILL.md that points to each file in '
    'reference_files ([{name, when_to_read, content}], written under references/), with the '
    'trigger queries its evaluation needs. draft: written, not yet run. evaluating: the eval '
    'loop holds it. accepted: beat the baseline and triggers correctly; the promoter '
    'ships it as the folder''s SKILL.md. rejected: kept with its eval for the record.';

COMMENT ON COLUMN memory.skill_drafts.eval_progress IS
    'The eval loop''s partial work while the draft is evaluating: the sibling skills it was '
    'compared against, the models used and the trigger queries answered so far. A pass does '
    'as many of the remaining checks as the language-model call budget allows and saves them '
    'here, so a draft that needs more calls than one tick can make is finished over several '
    'ticks. Cleared when the result is stored in eval.';
