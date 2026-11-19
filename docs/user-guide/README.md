# mike user guide

mike sits between Claude Code and Anthropic. It captures traffic routed through it for inspection in your browser. Optional features can change what is sent: lessons from earlier sessions, a nudge when a session drifts, a rewrite of the system prompt. You use it to see exactly what Claude Code did, to find a past turn again, and to stop correcting the same mistake twice.

This guide is for someone who has just installed mike. It follows one fictional example throughout. You are rewriting the install page of a small website in `~/projects/site`, in a Claude Code session named `docs` with the goal "Rewrite the install page". Halfway through you tell Claude: "Put Homebrew first on the install page." The pages show where that one correction appears: in the session's timeline, in a search, as a lesson, in a later session, and under a steerer or a filter rule.

## Why use mike

- **Read what Claude Code actually sent.** Open a session and step through it turn by turn: the system prompt, your messages, tool results and the streamed answer.
- **Find an earlier turn.** Search retained session history for the text in a request or an answer, in the browser or from inside Claude Code.
- **Keep a correction.** mike can turn "Put Homebrew first on the install page" into a lesson and offer it to the next session that needs it.
- **Catch drift and weak answers.** Responses that claim something they did not check, shift blame or skip the work are flagged. Optional steerers watch a session and send a correction when it wanders from its goal.
- **Run several sessions towards one goal.** Start Claude Code sessions from mike, see the messages between them, and follow a fleet in one place.

## Before you start

**A fresh configuration forwards to Anthropic without optional rewrites or background model calls.** Durable captures go to the configured PostgreSQL server, which can be on another machine. Trial session history stays in memory and is lost when mike stops; limits can evict it sooner. Optional features can send additional context to configured services. The **Watch only** preset restores feature-toggle defaults and disables session-lessons delivery, not every existing setting or filter rule. Review [preset scope](setup.md#choose-features) before applying it to an existing installation. [Settings](settings.md) lists the switches. [Privacy](privacy.md) says what each one sends and where.

**mike has no general login or multi-user isolation.** Local programs can read captures, change ordinary settings and start Claude Code sessions. Specific token-protected operations do not isolate one user from another. Install mike only on a computer that nobody else uses, and keep it on the local address it starts with. [Privacy](privacy.md) explains what that means in practice.

Captures can hold your code, tool output, prompts and secrets. Treat the database and its
backups like the project folders themselves. Capture is not guaranteed complete: routing,
size limits, stream completion, retention and storage availability affect what you can inspect.
`mike claude` configures routing, but another launcher can also route traffic through mike.

An empty Sessions page does not prove no traffic was stored. Alpha.5 forwards ordinary
proxy traffic without a usable session id without capture; newer source containing PR #185
can persist it separately in `requests.sessionless_requests`, not a session timeline.
Read [the version-scoped privacy limits](privacy.md#traffic-without-a-session-id) and
verify [a first session](quickstart.md) before relying on history.

mike runs on Linux x86_64 and ARM64 (glibc 2.35 or newer) and on Apple Silicon Macs. mike is an alpha, so expect rough edges. [Install mike](install.md) has the commands.

## Match the guide to your version

This guide follows the source checkout and can describe work newer than a published binary.
As checked on 19 November 2026, the newest published release is
[0.1.0-alpha.5](https://github.com/temet-ai/mike/releases/tag/v0.1.0-alpha.5), released on
8 November. It includes Linux ARM64 archives as well as Linux x86_64 and Apple Silicon macOS.

Check `mike --version` and `mike-mcp --version` before following a feature-specific path.
The installation examples select alpha.5. The basic setup and MCP commands also exist in
that tag, but reranking and the `unrecorded_captures` health ledger were added afterwards;
the relevant pages mark those differences. Do not expect those controls in alpha.5 or
add newer configuration keys to it. For the original release documentation, use the
[guide at the alpha.5 tag](https://github.com/temet-ai/mike/tree/v0.1.0-alpha.5/docs/user-guide).
This is a source comparison, not a claim that every journey has been exercised on a release binary.

## Where to start

1. [Install mike](install.md) with Homebrew, mise or an archive.
2. [Set mike up](setup.md): choose where history is kept, register Claude Code and pick the features you want.
3. [Capture your first session](quickstart.md) and open it in the browser.
4. [Find a page](pages.md) when you want the guide for a screen or a command.

## Choose your next path

Start with the result you want. You do not need to turn on every feature or read the guide
from beginning to end.

| I want to… | Follow this path | Check that it worked |
|---|---|---|
| Understand a surprising answer | [Read the turn and request](reading-sessions.md#look-at-one-request), then inspect [the prompts and tools](prompts-and-agents.md) | You can point to what Claude received and what it did next. A flagged response is a clue, not proof. |
| Stop repeating a correction | [Record a lesson](memory.md#keep-a-correction-as-a-lesson), then [enable delivery](memory.md#get-a-lesson-into-the-next-session) | Verify it was stored, then inspect whether a relevant later request actually received it. |
| Ask Claude about earlier work | [Register MCP](mcp.md#register-it), then [search the example session](mcp.md#ask-about-the-example-session) | The answer leads back to a captured session and request, rather than an unsupported recollection. |
| Split work across sessions | [Preview a fleet](fleets/first-fleet.md), review [its definition](fleets/definition.md), then [watch the run](fleets/watch-and-recover.md) | Each task has evidence and an acceptance result; opening several sessions is only the start. |
| Keep history through an upgrade | Read [backup and recovery](backup-and-recovery.md) before upgrading | A verified backup is one checkpoint; an isolated restore and a new captured session establish more. Check the page's tested/not-rehearsed limits. |
| Find missing history | [Check capture health](troubleshooting.md#check-that-history-is-being-saved) | A new prompt appears in history. Claude answering on its own does not prove mike saved it. |

## The pages of this guide

**Start**

- [Capture your first session](quickstart.md): from an installed mike to a sample session you can inspect in your browser.
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
- [Decisions](decisions.md): a small model that answers mike's quick yes or no questions.
- [Filter rules](filters.md): rewrite parts of the system prompt before it is sent.
- [Compression](compression.md): what mike can shorten, and what it does today.

**Work with sessions**

- [MCP](mcp.md): let Claude Code query what mike has captured.
- [Start and manage sessions](sessions.md): open Claude Code sessions from mike.
- [Work threads](threads.md): keep work going across sessions.
- [Mods](mods.md): the panes and buttons mike adds inside Claude Code.
- [Codex sessions](codex.md): what mike sees of OpenAI Codex, and how to connect it.

**Fleets**

- [Fleets: several sessions on one goal](fleets/README.md): what a fleet is, when to use one, and what you need first.
- [Run your first fleet](fleets/first-fleet.md): the example from `/fleet` to `done`.
- [Write a fleet definition](fleets/definition.md): the outcomes, the tasks and their files, and the checks.
- [How a fleet runs](fleets/how-a-fleet-runs.md): the sessions mike opens and what each one does.
- [How the sessions work together](fleets/working-together.md): what each session can learn about the others, questions, messages and shared decisions.
- [Share one checkout](fleets/shared-checkout.md): commits, the rules for the folder, and checks on clean copies.
- [How work is accepted](fleets/acceptance.md): the report, the floor and the judge.
- [Watch a fleet and step in](fleets/watch-and-recover.md): the status screen, questions for you, repairs and stopping.
- [After a run](fleets/after-a-run.md): the record, the retrospective and lessons for the next fleet.

**Reference**

- [Command line](command-line.md): every `mike` command.
- [Settings](settings.md): what you can change, where, and the defaults.
- [Privacy](privacy.md): what mike stores, rewrites and sends.
- [Backup and recovery](backup-and-recovery.md): keep your history and move it to a new machine.
- [Troubleshooting](troubleshooting.md): when no session appears or something looks wrong.
