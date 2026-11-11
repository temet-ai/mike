## What a thread is

A thread is one piece of work in a project that takes more than one session. mike's database holds it, so every machine with a location for the project sees the same threads. Open [Threads](/threads), pick a project, then pick a thread from the list.

A thread has an identity (title, goal, status, branch, worktree), a current state, acceptance criteria, dependencies on other threads, the sessions that worked on it, and a history.

## Status

- **Active**: work is being planned or has just started.
- **Implementing**: sessions are building it.
- **Review**: the work is done and waits for someone to check it against the acceptance criteria. A thread needs acceptance criteria before it can move to review.
- **Parked**: set aside on purpose. The state says how to restart it.
- **Done**: finished.

The status filter above the list narrows it to one of these.

## State and checkpoints

The state is what the last checkpoint wrote: a short summary, where the work stands, what comes next, blockers, open questions, decisions and the files the next session should read first. Each checkpoint replaces the state as a whole. The state it replaced stays in the history, so nothing is lost.

## History

The history is append-only. Each entry has a kind (checkpoint, decision, finding, brief, handoff or note), the date, the session that wrote it and the commit it was written at. Click the History heading to open it, and click an entry to read it in full. A brief one session writes for another is an entry here, so the next session reads it from mike on any machine.

## Editing

Use **Edit** on a thread. Three things can change:

- **Details**: title, goal, status, branch, worktree, dependencies and acceptance criteria. Only the fields you change are sent.
- **Checkpoint**: writes a new state. The summary is required.
- **Add entry**: appends a note, decision, finding, brief or handoff to the history.

A refused change, such as moving to review with no acceptance criteria, shows mike's reason under the form and changes nothing.

## Threads that failed to import

A thread imported from an older worklog file that mike could not read completely shows an import problem under its title. Its other fields hold what was read.
