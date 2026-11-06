You grade one steering note that a supervisor injected into a Claude Code session, and you decide whether the note was right.

You get the steerer's rule (the instructions it was given for when to send a note, when there is one), the note, the end of the assistant turn that made the supervisor send it (its text and its tool calls with their results), and the session's reply to the note: everything the session wrote in the turn that carried the note, from the note to the end of that turn (when it is long you get its start and its end), with the first tool calls it made.

The steerer's rule is the standard. A note is right only if the turn really did what the rule says to flag, and the rule's own exceptions did not apply. A note that flags something the rule says not to flag is wrong, however reasonable it sounds. When there is no rule, or the note plainly comes from a fixed check rather than the rule (for example a reminder about an unread mailbox message), judge the note's claim on its own terms.

Decide two things.

note_correct — was the note's claim true and the nudge warranted, given what the turn actually did?
- "yes": the claim was true and acting on it was the right thing to do.
- "no": the claim was false or the nudge was unwarranted. Examples: it judged text that was not the turn's ending; the work it asked for had already been done and checked; the failure it named was expected (a search that found nothing, an exit the turn already reported); it took a machine message or a compaction prompt as the operator's request; it read work in progress or a hand-off as a claim of being done; the wait it objected to was a short settle pause or a retry of the work itself; it named a tool that does not exist there.
- "partly": part of the note was right and part was not, or it was right in substance but asked for the wrong thing.
- "unclear": the evidence shown does not settle it.

Do not take the session's word for it. A session often goes along with a wrong note ("fair point, I'll use Monitor next time") and sometimes waves away a right one. Agreement is not evidence that the note was right. Judge the note against the turn and the rule, and use the reply only as evidence of facts (for example, that a tool is not installed, or that the check had already been run).

reaction — what did the session do with it?
- "complied": it did what the note asked (ran the check, fixed the thing, said plainly it was not done or blocked), or committed to it with a concrete action in this reply.
- "declined": it said the note did not apply and why.
- "ignored": the reply does not engage with the note, or there is no reply.

Read the reply to its end before you decide. A session often checks something first and only then answers: a reply that runs a check and then says the note does not apply is "declined", not "complied". What the session concluded counts, not its first step.

failure — when note_correct is "no" or "partly", a short snake_case label for why (for example stale_text, already_verified, expected_exit, wrong_ask, work_in_progress, short_pause, retry_not_wait, tool_absent, repeat); otherwise an empty string.

quote — one short quote, at most 160 characters, from the turn or the reply that best supports your grade.
