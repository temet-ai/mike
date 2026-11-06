# Add mike to Claude Code with mods

A mod is a small add-on that Claude Code loads into its own terminal screen. mike ships
five that it installs for you: a sessions pane and mail notices, a pane for a fleet's
state, a reply route for mike's messages, a list of the mods in a session, and work
threads. They put mike's information where you are already typing, instead of in the
[web UI](web-ui.md). Mods change how Claude Code looks and what it offers you; none changes
what mike captures, and none is needed for mike to work.

A sixth, `wiretap`, ships but is off unless you ask for it. This page covers only what an
installed mike carries.

## Install them

The setup wizard offers them on its Claude Code step ([Setup](setup.md)). To do it from a
terminal:

```sh
mike claude-code mods
```

This adds the mods' folders to `CLAUDE_CODE_PLUGIN_DIRS` in the `env` section of
`~/.claude/settings.json`. Every other setting and every folder already listed stays, and
mike records what it added so that `mike uninstall` removes only that. Restart Claude Code
afterwards.

mike finds the mods in `share/mike/mods` next to its binary, which is where a Homebrew or
archive install keeps them (see [Install](install.md)). If it finds none, setup says
"No folder of mods was found beside this mike binary or in a checkout" and the command
installs nothing.

Check the result with `mike claude-code status`; its `mods:` line says how many folders are
listed.

## Turn mods off

- **All of them:** `mike claude-code mods --remove` takes out the folders mike added and
  nothing else.
- **One:** edit `CLAUDE_CODE_PLUGIN_DIRS` in `~/.claude/settings.json` and delete that
  mod's folder from the colon-separated list, then restart Claude Code. Your own entries
  in the list are never touched by mike.
- **For one session:** none; Claude Code reads the list when it starts.

Running `mike claude-code mods` again after a removal puts them back.

## See what is loaded

Type `/mods` in Claude Code. A pane lists each mod the session loaded, with its commands
(click one to run it), what it adds and what it does on its own, and a filter box.
`/mods text` prints the same as text. Under the prompt, the mods that offer a button show
as one row of chips (for example **Mods**, **Fleet**, **Sessions**); the **Hide** chip puts
the row away and `/mods buttons` brings it back.

The `mods` mod needs nothing from mike.

## What each mod adds

### mike

Shows mike's view of your project inside Claude Code. `/seats` opens a pane of the project's
sessions. The status line shows unread mike mail and changes other sessions made to your
checkout. `/recall` searches past turns. With the matching setting on, it also adds fitting
[lessons](memory.md) to a prompt, and it tells mike which mods the session loaded.

With the fictional `docs` session in `~/projects/site`: open `/seats` and the pane lists
`docs` with any other session in that project; `/recall Homebrew first` finds the turn where
you corrected Claude.

### fleet-comms

Works only in a session that mike started for a fleet; any other session loads it and gets
nothing from it. `/fleet-comms` opens a pane with the fleet's state as
`mike fleet status` draws it, then its messages. While the fleet runs, a band above the prompt
carries the latest message and **NEEDS YOU**, and the status line says **NEEDS YOU** or
**NOTHING IS MOVING**. A fleet that is done, failed or not yet started raises nothing.
See [Fleets](fleets.md).

In a fleet session whose task has a goal, the mod starts Claude Code's `/goal` with that
goal after the session's first turn.

### mike-mail

Sends your reply to a session that wrote to you through mike back through mike's mailbox,
instead of Claude Code's own route, with a notice. This keeps the thread and delivery
receipts visible in [Sessions](reading-sessions.md).

### mods

The `/mods` list described above.

### thread

Work threads held in mike, so a multi-session job survives a restart: `/thread:list`,
`/thread:get`, `/thread:new`, `/thread:checkpoint`, `/thread:depend`, and `/thread-board`
for a pane of the project's open threads grouped by status. The first prompt of a session
carries the project's open threads, one line each, at most 2 KB. Typing `#` and part of a
slug offers the threads. See [Threads](threads.md).

## What is not installed

- **wiretap** records prompts, system-prompt parts, model steps and tool calls to a JSON
  file for comparison with mike's capture. It writes everything a session sees to disk, so
  it is installed only with `mike claude-code mods --with-wiretap`. See
  [Privacy](privacy.md).
- Nothing else. The mods work on their own; they need no other add-on.

## When nothing shows

| What you see | Next step |
|---|---|
| `/mods` or `/seats` is not a command | Run `mike claude-code status`; if `mods:` is missing, run `mike claude-code mods` and restart Claude Code |
| "No folder of mods was found" | Install mike from a Homebrew or archive build ([Install](install.md)); a build without the mods has nothing to install |
| `/fleet-comms` does nothing | It is only for a session mike started for a fleet |
| Status line shows no mail | The mod needs a running mike; see [Troubleshooting](troubleshooting.md) |
