# mike user guide

mike sits between Claude Code and Anthropic. It records every request Claude Code sends and every answer that comes back, shows them in your browser, and can add to them: lessons from earlier sessions, a nudge when a session drifts, a rewrite of the system prompt. You use it to see exactly what Claude Code did, to find a past turn again, and to stop correcting the same mistake twice.

This guide is for someone who has just installed mike. It follows one fictional example throughout. You are rewriting the install page of a small website in `~/projects/site`, in a Claude Code session named `docs` with the goal "Rewrite the install page". Halfway through you tell Claude: "Put Homebrew first on the install page." The pages show where that one correction appears: in the session's timeline, in a search, as a lesson, in a later session, and under a steerer or a filter rule.

## Why use mike

- **Read what Claude Code actually sent.** Open a session and step through it turn by turn: the system prompt, your messages, tool results and the streamed answer.
- **Find an earlier turn.** Search every project and session for the text in a request or an answer, in the browser or from inside Claude Code.
- **Keep a correction.** mike can turn "Put Homebrew first on the install page" into a lesson and offer it to the next session that needs it.
- **Catch drift and weak answers.** Responses that claim something they did not check, shift blame or skip the work are flagged. Optional steerers watch a session and send a correction when it wanders from its goal.
- **Run several sessions towards one goal.** Start Claude Code sessions from mike, see the messages between them, and follow a fleet in one place.

## Before you start

**A fresh install only captures.** mike forwards what Claude Code sent, returns what Anthropic answered, and records both on your machine. Features that rewrite your requests, or that send captured text to another service, are off until you turn them on, and the setup step that offers a preset says what each choice does. [Settings](settings.md) lists the switches. [Privacy](privacy.md) says what each one sends and where.

**mike has no login.** Any program on your computer can read every captured prompt and answer, change mike's settings and start Claude Code sessions. Install mike only on a computer that nobody else uses, and keep it on the local address it starts with. [Privacy](privacy.md) explains what that means in practice.

Captured sessions hold your code, tool output and anything you typed. Treat the database and its backups like the project folders themselves.

mike runs on Linux x86_64 and ARM64 (glibc 2.35 or newer) and on Apple Silicon Macs. mike is an alpha, so expect rough edges. [Install mike](install.md) has the commands.

## Where to start

1. [Install mike](install.md) with Homebrew, mise or an archive.
2. [Set mike up](setup.md): choose where history is kept, register Claude Code and pick the features you want.
3. [Capture your first session](quickstart.md) and open it in the browser.
4. [Find a page](pages.md) when you want the guide for a screen or a command.

## The pages of this guide

**Start**

- [Capture your first session](quickstart.md): from nothing installed to one request open in your browser.
- [Install mike](install.md): Homebrew, mise or an archive, and what each gives you.
- [Set mike up](setup.md): the first-run steps, the database, the background service and Claude Code.
- [Find a page](pages.md): every screen and command, with the page that explains it.

**Watch**

- [Get around the web UI](web-ui.md): the menu, search, Help and how the pages are organised.
- [Read a session](reading-sessions.md): sessions, turns, requests and the messages between sessions.
- [Projects](projects.md): sessions grouped by project, and the knowledge kept for each.
- [Flagged responses](flagged-responses.md): unbacked claims, blame-shifting and avoided work.
- [Prompts, agents and tools](prompts-and-agents.md): the system prompts, subagents and tool calls mike saw.
- [The terminal app](terminal.md): run `mike` in a terminal instead of the browser.

**Change what is sent**

- [Memory](memory.md): lessons, recall and reminders that carry a correction to later sessions.
- [Steering](steering.md): steerers that watch a session and send a correction.
- [The judge](judge.md): how mike labels turns and sessions, and how you check it.
- [Filter rules](filters.md): rewrite parts of the system prompt before it is sent.
- [Compression](compression.md): what mike can shorten, and what it does today.

**Work with sessions**

- [MCP](mcp.md): let Claude Code query what mike has captured.
- [Start and manage sessions](sessions.md): open Claude Code sessions from mike.
- [Fleets](fleets/README.md): several sessions working towards one goal.
- [How a fleet runs](fleets/how-a-fleet-runs.md): what mike does from the moment a fleet starts until it closes, and what each session in it does.
- [Work threads](threads.md): keep work going across sessions.
- [Mods](mods.md): the panes and buttons mike adds inside Claude Code.

**Reference**

- [Command line](command-line.md): every `mike` command.
- [Settings](settings.md): what you can change, where, and the defaults.
- [Privacy](privacy.md): what mike stores, rewrites and sends.
- [Backup and recovery](backup-and-recovery.md): keep your history and move it to a new machine.
- [Troubleshooting](troubleshooting.md): when no session appears or something looks wrong.
