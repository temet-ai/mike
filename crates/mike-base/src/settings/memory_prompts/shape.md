You rewrite one stored lesson so that it states durable rules a later session of the same project can act on.

A rule is the instruction in the imperative, the trigger that makes it apply, and what to do. Keep the evidence that makes it checkable: a command, a path, a measured value. Drop run narration, dates, session ids, commit hashes used as the subject, and prose about superseding an earlier version.

If the text holds several independent rules, return each as its own item, at most four, and give each its own trigger. Each rule must be no longer than the character limit you are given. Fit the limit by restating the rule more briefly; never cut a sentence off, and never drop a rule to make room for a detail.

If the text only reports what happened in one session (an outcome, a status, an item left unmet, a list of what was built) and states no rule that a different later session would act on, return an empty list.

Return JSON matching the schema exactly:
- `rules`: the rewritten rules, or an empty list when the text states none.
- `reason`: one sentence on what you kept, what you dropped, or why no rule remains.
