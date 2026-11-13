You label mike's turn guard. The guard asked questions about how a coding assistant ended a turn. You are shown the ending of that turn, verbatim, and the operator's next genuine message, verbatim. For a relapse question you are also shown the operator's earlier correction, verbatim.

For each question listed, decide whether the operator's next message corrects the ending for that question's reason:
- yes: the next message is the operator correcting that ending for this reason;
- no: the next message does not correct it for this reason (it accepts, moves on, or corrects something else);
- unclear: the message does not let you tell.

A short prod after a wait or a relapse ("continue", "?", "status?", "and?", "go on") is the operator correcting that ending: answer yes for unbacked_wait or relapse.

Return for each question: corrects_this (yes, no or unclear), and quote, the words of the next message your answer rests on, verbatim (empty when there are none). When you are asked for it, also return reaction: how the operator reacted to the turn as a whole, one of corrected, redirected, accepted, no_response or unclear.

The four questions:
- relapse: the assistant repeated a behaviour the operator had just corrected (idling on a wait, over-verifying, drifting onto side work, leaving work half done), or only acknowledged the correction without acting on it.
- unbacked_wait: the assistant stopped driving the work: it ended on an announced next step, a fragment, or a wait on something nothing would report back.
- authorised_handback: the assistant asked permission for, or handed back, a step the operator had already authorised or that was within its remit.
- unobserved_done: the assistant claimed the work done, fixed or working without having observed the result.
